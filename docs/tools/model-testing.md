# Model Testing

## The moment

A new model appears and everyone has an opinion about it. What matters is how it handles your actual task. Put the same prompt in front of a council, compare the answers, and let a separate panel judge them instead of relying on a benchmark that measures somebody else's workload.

## How it fits your day

Use a repeatable test before changing the model that supports your daily work. Keep the prompt, judge instructions, and report together so the next model is compared against the same question rather than a memory of the last result.

## What it can do

- [Which model is actually better at my task?](../features.md#which-model-is-actually-better-at-my-task)
- [Let a panel score the answers](../features.md#let-a-panel-score-the-answers)
- [Local models can judge too](../features.md#local-models-can-judge-too)
- [One Markdown artifact you can keep](../features.md#one-markdown-artifact-you-can-keep)
- [Stop retyping the same test prompts](../features.md#stop-retyping-the-same-test-prompts)
- [Test vision models with a real image](../features.md#test-vision-models-with-a-real-image)
- [Thinking on or off, per model](../features.md#thinking-on-or-off-per-model)

## Settings that matter

- [Model Testing settings](../reference/settings/model-testing.md)
- [Advanced settings](../reference/settings/advanced.md)

## When it goes wrong

- [Unconfigured seats stay out of the run](../features.md#unconfigured-seats-stay-out-of-the-run)
- [See what's happening between dispatch and verdict](../features.md#see-whats-happening-between-dispatch-and-verdict)
- [A hung provider can't stall the app](../features.md#a-hung-provider-cant-stall-the-app)

## Set it up

1. Choose the participating providers at `Model Testing › Models`.
2. Enter one task all candidates should answer at `Model Testing › Prompt for all models`.
3. Add an image only when the task needs one at `Model Testing › Image (optional, for vision models)`.
4. Add judging instructions at `Model Testing › Judge / arbiter prompt (optional)`.
5. Enable scoring at `Model Testing › Judge = On`.
6. Start the comparison at `Model Testing › Run test` and inspect the results.
7. Preserve the artifact from `Model Testing › Save as…`.
