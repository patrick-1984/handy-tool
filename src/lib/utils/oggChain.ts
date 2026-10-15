/**
 * A long take is glued from its 10-minute chunks as a CHAINED Ogg file: one
 * Opus stream per chunk, back to back (see opus_chunk.rs). Handy's own decoder
 * reads every stream, but the web view's <audio> element plays and seeks only
 * the first one - an 11:43 take stopped at 10:09. `toSingleOggStream` rewrites
 * such a file, in memory, as ONE continuous Opus stream: the later streams'
 * header pages (OpusHead, OpusTags) are dropped, every page takes the first
 * stream's serial and the next page number, granule positions are rebuilt
 * from the Opus packets' own durations on one timeline (end trimming only on
 * the very last page), and each page gets a fresh CRC.
 *
 * Only a file laid out exactly as Handy writes it is rewritten: valid CRCs,
 * mono Opus (mapping family 0), each header packet complete on its own page,
 * and every stream's granules equal to its pre-skip plus its packets' samples.
 * Anything else is returned unchanged.
 */

interface Page {
  start: number;
  end: number;
  headerType: number;
  granule: bigint;
  serial: number;
  sequence: number;
  /** Lacing values, and where the body starts. */
  lacing: Uint8Array;
  bodyStart: number;
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let r = i << 24;
    for (let j = 0; j < 8; j++) {
      r = r & 0x80000000 ? (r << 1) ^ 0x04c11db7 : r << 1;
    }
    table[i] = r >>> 0;
  }
  return table;
})();

/** The Ogg page checksum (CRC-32, polynomial 0x04c11db7, no reflection),
 *  computed with the page's own checksum field taken as zero. */
export const oggPageCrc = (page: Uint8Array): number => {
  let crc = 0;
  for (let i = 0; i < page.length; i++) {
    const byte = i >= 22 && i < 26 ? 0 : page[i];
    crc = ((crc << 8) ^ CRC_TABLE[((crc >>> 24) ^ byte) & 0xff]) >>> 0;
  }
  return crc >>> 0;
};

const parsePages = (data: Uint8Array): Page[] | null => {
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  const pages: Page[] = [];
  let pos = 0;
  while (pos < data.length) {
    if (
      pos + 27 > data.length ||
      data[pos] !== 0x4f || // "OggS"
      data[pos + 1] !== 0x67 ||
      data[pos + 2] !== 0x67 ||
      data[pos + 3] !== 0x53 ||
      data[pos + 4] !== 0 // stream structure version
    ) {
      return null;
    }
    const segments = data[pos + 26];
    const bodyStart = pos + 27 + segments;
    if (bodyStart > data.length) return null;
    const lacing = data.subarray(pos + 27, bodyStart);
    let bodyLength = 0;
    for (const value of lacing) bodyLength += value;
    const end = bodyStart + bodyLength;
    if (end > data.length) return null;
    if (
      oggPageCrc(data.subarray(pos, end)) !== view.getUint32(pos + 22, true)
    ) {
      return null; // damaged: leave the file alone
    }
    pages.push({
      start: pos,
      end,
      headerType: data[pos + 5],
      granule: view.getBigInt64(pos + 6, true),
      serial: view.getUint32(pos + 14, true),
      sequence: view.getUint32(pos + 18, true),
      lacing,
      bodyStart,
    });
    pos = end;
  }
  return pages;
};

const startsWith = (data: Uint8Array, at: number, text: string): boolean => {
  for (let i = 0; i < text.length; i++) {
    if (data[at + i] !== text.charCodeAt(i)) return false;
  }
  return true;
};

/** A page holding exactly one complete packet (a header page). */
const singlePacketPage = (page: Page): boolean =>
  (page.headerType & 0x01) === 0 &&
  page.lacing.length > 0 &&
  page.lacing[page.lacing.length - 1] < 255 &&
  page.lacing.subarray(0, -1).every((v) => v === 255);

/** The OpusTags packet Handy writes (opus_chunk.rs `opus_tags`): vendor
 *  "handy", no comments. */
const HANDY_TAGS = new Uint8Array([
  ...new TextEncoder().encode("OpusTags"),
  5,
  0,
  0,
  0,
  ...new TextEncoder().encode("handy"),
  0,
  0,
  0,
  0,
]);

const sameBytes = (data: Uint8Array, at: number, expected: Uint8Array) =>
  expected.every((byte, i) => data[at + i] === byte);

/** Samples (at 48 kHz) a complete Opus packet decodes to; null for one
 *  Handy does not write. Handy's encoder emits one frame per packet (TOC
 *  code 0, RFC 6716 §3.2.2) - multi-frame packets (codes 1-3) are not
 *  recognised, so a file holding one is left unchanged. `toc` is the packet's
 *  first byte, `length` its size. */
const opusPacketSamples = (toc: number, length: number): number | null => {
  if (length < 1 || length - 1 > 1275 || (toc & 0x03) !== 0) return null;
  const config = toc >> 3;
  return config < 12
    ? [480, 960, 1920, 2880][config % 4]
    : config < 16
      ? [480, 960][config % 2]
      : [120, 240, 480, 960][config % 4];
};

interface Stream {
  pages: Page[];
  preSkip: number;
  /** Per audio page (index 2 on): samples of the packets completed on it. */
  pageSamples: number[];
  /** Samples trimmed off the end by the stream's last granule. */
  endTrim: number;
}

/** Checks one stream against the exact layout Handy writes and measures its
 *  audio pages; null when anything does not match. */
const readStream = (
  data: Uint8Array,
  pages: Page[],
  firstHead: Page,
): Stream | null => {
  const [head, tags] = pages;
  if (
    pages.length < 3 ||
    !singlePacketPage(head) ||
    !singlePacketPage(tags) ||
    head.granule !== 0n ||
    tags.granule !== 0n ||
    head.end - head.bodyStart !== 19 || // mapping family 0: exactly 19 bytes
    !startsWith(data, head.bodyStart, "OpusHead") ||
    data[head.bodyStart + 8] !== 1 || // version 1
    data[head.bodyStart + 9] !== 1 || // mono
    data[head.bodyStart + 16] !== 0 || // output gain 0
    data[head.bodyStart + 17] !== 0 ||
    data[head.bodyStart + 18] !== 0 || // mapping family 0
    // the same input rate as the first stream (bytes 12..15)
    [12, 13, 14, 15].some(
      (i) => data[head.bodyStart + i] !== data[firstHead.bodyStart + i],
    ) ||
    tags.end - tags.bodyStart !== HANDY_TAGS.length ||
    !sameBytes(data, tags.bodyStart, HANDY_TAGS)
  ) {
    return null;
  }
  for (let i = 0; i < pages.length; i++) {
    const type = pages[i].headerType;
    if (pages[i].sequence !== i || (type & 0xf8) !== 0) return null;
    if (((type & 0x02) !== 0) !== (i === 0)) return null; // BOS: first only
    if (((type & 0x04) !== 0) !== (i === pages.length - 1)) return null; // EOS
  }
  const preSkip = data[head.bodyStart + 10] | (data[head.bodyStart + 11] << 8);

  const pageSamples: number[] = [];
  let total = preSkip;
  let inPacket = false; // the previous page ended inside a packet
  let toc = 0;
  let packetLength = 0;
  for (const page of pages.slice(2)) {
    // The continuation flag must say exactly whether a packet carries over.
    if (((page.headerType & 0x01) !== 0) !== inPacket) return null;
    let samples = 0;
    let completed = 0;
    let offset = page.bodyStart;
    for (const value of page.lacing) {
      if (!inPacket) {
        toc = data[offset]; // (an empty packet is rejected by its length)
        packetLength = 0;
      }
      packetLength += value;
      offset += value;
      inPacket = value === 255;
      if (!inPacket) {
        const n = opusPacketSamples(toc, packetLength);
        if (n === null) return null;
        samples += n;
        completed++;
      }
    }
    total += samples;
    pageSamples.push(samples);
    const last = page === pages[pages.length - 1];
    if (completed === 0) {
      // Mid-stream: a page that only carries part of a packet (granule -1).
      // Last: the empty end-of-stream page Handy's writer emits when a take
      // stops right after a page was flushed (granule = the stream's end).
      const expected = last ? BigInt(total) : -1n;
      if (page.granule !== expected) return null;
    } else if (!last && page.granule !== BigInt(total)) {
      return null; // granules are not what Handy writes
    }
  }
  if (inPacket) return null; // the stream ends inside a packet
  const endGranule = pages[pages.length - 1].granule;
  const endTrim = total - Number(endGranule);
  const lastSamples = pageSamples[pageSamples.length - 1];
  if (endGranule < 0n || endTrim < 0 || endTrim > lastSamples) return null;
  return { pages, preSkip, pageSamples, endTrim };
};

export const toSingleOggStream = (data: Uint8Array): Uint8Array => {
  const pages = parsePages(data);
  if (!pages || pages.length === 0) return data;

  // Split into consecutive streams: a chained file has each stream complete
  // before the next begins.
  const groups: Page[][] = [];
  for (const page of pages) {
    const current = groups[groups.length - 1];
    if (!current || current[0].serial !== page.serial) {
      if (groups.some((g) => g[0].serial === page.serial)) return data;
      groups.push([page]);
    } else {
      current.push(page);
    }
  }
  if (groups.length < 2) return data;

  const streams: Stream[] = [];
  for (const group of groups) {
    const stream = readStream(data, group, groups[0][0]);
    if (!stream) return data;
    streams.push(stream);
  }

  const out = new Uint8Array(data.length);
  const serial = streams[0].pages[0].serial;
  let length = 0;
  let sequence = 0;
  // One timeline: the first stream's pre-skip, then every packet's samples.
  // Later streams' pre-skip (encoder warm-up) and the earlier streams' end
  // padding are played as part of the timeline; only the last page trims.
  let granule = BigInt(streams[0].preSkip);
  const lastStream = streams.length - 1;

  streams.forEach((stream, index) => {
    const kept = index === 0 ? stream.pages : stream.pages.slice(2);
    kept.forEach((page, keptIndex) => {
      const finalPage = index === lastStream && keptIndex === kept.length - 1;
      // An earlier stream's empty end-of-stream page carries nothing: drop it.
      if (page.lacing.length === 0 && !finalPage) return;
      const bytes = out.subarray(length, length + (page.end - page.start));
      bytes.set(data.subarray(page.start, page.end));
      const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.length);
      const audioIndex = index === 0 ? keptIndex - 2 : keptIndex;
      let headerType = page.headerType & ~0x04;
      if (index > 0) headerType &= ~0x02;
      if (finalPage) headerType |= 0x04;
      bytes[5] = headerType;
      if (audioIndex >= 0) {
        const samples = stream.pageSamples[audioIndex];
        granule += BigInt(samples);
        const value = finalPage
          ? granule - BigInt(stream.endTrim)
          : samples === 0
            ? -1n
            : granule;
        view.setBigInt64(6, value, true);
      }
      view.setUint32(14, serial, true);
      view.setUint32(18, sequence++, true);
      view.setUint32(22, oggPageCrc(bytes), true);
      length += bytes.length;
    });
  });

  return out.subarray(0, length);
};
