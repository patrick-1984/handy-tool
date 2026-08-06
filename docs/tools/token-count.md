# Token Count

## The moment

You are about to paste a 40 KB prompt into several providers. Before committing it, you want to know what each provider will count and what that implies for the context you are spending.

## How it fits your day

Use Token Count as a preflight for long prompts and files. Start with an offline count when you need a quick estimate; invoke configured providers only when their own accounting is the question, because provider counting sends the complete text to those services.

## What it can do

- [What will this prompt cost?](../features.md#what-will-this-prompt-cost)
- [Counts without a network call](../features.md#counts-without-a-network-call)
- [One click, every provider, one table](../features.md#one-click-every-provider-one-table)
- [Count a file instead of pasting it](../features.md#count-a-file-instead-of-pasting-it)
- [Token counts you can trust from a local server](../features.md#token-counts-you-can-trust-from-a-local-server)
- [Configure a provider once, use it everywhere](../features.md#configure-a-provider-once-use-it-everywhere)

## Settings that matter

- [Token Count settings](../reference/settings/token-count.md)
- [Advanced settings](../reference/settings/advanced.md)

## When it goes wrong

- [A hung provider can't stall the app](../features.md#a-hung-provider-cant-stall-the-app)
- [Several slots, one local loader](../features.md#several-slots-one-local-loader)

## Set it up

1. Paste the prompt at `Token Count › Paste text here to count tokens...`.
2. For an offline GPT-4 baseline, choose `Token Count › cl100k (GPT-4)`.
3. For an offline GPT-4o baseline, choose `Token Count › o200k (GPT-4o)`.
4. To compare enabled services, run `Token Count › Count with all`.
5. Use `Token Count › Count with all (parallel)` only when the configured providers can accept concurrent work.
6. For a saved prompt, begin at `Token Count › Open file...`.
