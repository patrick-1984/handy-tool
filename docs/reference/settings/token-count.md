# Token Count settings

Open `Token Count`. The page has no persisted settings of its own; provider chips come from Registered LLM Providers.

### Paste text here to count tokens...

`Token Count › Paste text here to count tokens...`

Holds the text to count. **Default:** empty.

Catalog: [What will this prompt cost?](../../features.md#what-will-this-prompt-cost).

### cl100k (GPT-4)

`Token Count › cl100k (GPT-4)`

Counts the current text locally with `cl100k_base`. **Default:** available; no count until clicked.

Catalog: [Counts without a network call](../../features.md#counts-without-a-network-call).

### o200k (GPT-4o)

`Token Count › o200k (GPT-4o)`

Counts the current text locally with `o200k_base`. **Default:** available; no count until clicked.

Catalog: [Counts without a network call](../../features.md#counts-without-a-network-call).

### Estimate

`Token Count › Estimate`

Produces the built-in rough local estimate; dynamic provider chips follow it. **Default:** available; excluded from the exact-tokenizer difference baseline.

Catalog: [Counts without a network call](../../features.md#counts-without-a-network-call).

### Count with all

`Token Count › Count with all`

Runs enabled counters serially, which suits provider entries sharing one local loader. **Default:** idle.

Catalog: [One click, every provider, one table](../../features.md#one-click-every-provider-one-table).

### Count with all (parallel)

`Token Count › Count with all (parallel)`

Runs enabled counters concurrently where their provider configuration allows it. **Default:** idle.

Catalog: [One click, every provider, one table](../../features.md#one-click-every-provider-one-table).

### Cancel

`Token Count › Cancel`

Stops an active all-provider sweep. **Default:** hidden while no sweep is running.

### Open file...

`Token Count › Open file...`

Loads a text file, up to the supported 10 MB limit, for counting. **Default:** no file selected.

Catalog: [Count a file instead of pasting it](../../features.md#count-a-file-instead-of-pasting-it).
