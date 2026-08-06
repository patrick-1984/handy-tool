# Model Testing settings

Open `Model Testing`. Prompt text, images, and run selections are working state; saved prompts, images, and presets become part of the model-test library.

### Models

`Model Testing › Models`

Lists enabled registered providers and provides per-provider run and judge choices. **Default:** the enabled, configured provider set; no rows selected for a new run.

Catalog: [Which model is actually better at my task?](../../features.md#which-model-is-actually-better-at-my-task).

### Select all to run

`Model Testing › Select all to run`

Selects every eligible provider's [Run](#run) checkbox. **Default:** not applicable; this is an action.

Catalog: [Which model is actually better at my task?](../../features.md#which-model-is-actually-better-at-my-task).

### Clear

`Model Testing › Clear`

Clears provider run and judge selections. **Default:** not applicable; this is an action.

### Run

`Model Testing › Run`

Includes that provider as a model under test. **Default:** Off for each provider row.

Catalog: [Which model is actually better at my task?](../../features.md#which-model-is-actually-better-at-my-task).

### Judge

`Model Testing › Judge`

Includes that provider in the judge panel. This is the per-provider checkbox, separate from the later Judge parameter row. **Default:** Off for each provider row.

Catalog: [Let a panel score the answers](../../features.md#let-a-panel-score-the-answers).

### Preset

`Model Testing › Preset`

Loads a saved pair of model and judge prompts. **Default:** no preset loaded; a fresh library is empty.

Catalog: [Stop retyping the same test prompts](../../features.md#stop-retyping-the-same-test-prompts).

### Prompt for all models

`Model Testing › Prompt for all models`

Sets the prompt sent to every selected [Run](#run) provider. **Default:** empty.

Catalog: [Which model is actually better at my task?](../../features.md#which-model-is-actually-better-at-my-task).

### Image (optional, for vision models)

`Model Testing › Image (optional, for vision models)`

Attaches an image by click or drag-and-drop for providers that accept vision input. **Default:** no image.

Catalog: [Test vision models with a real image](../../features.md#test-vision-models-with-a-real-image).

### Judge / arbiter prompt (optional)

`Model Testing › Judge / arbiter prompt (optional)`

Sets the instructions used by selected judge providers. **Default:** empty.

Catalog: [Let a panel score the answers](../../features.md#let-a-panel-score-the-answers).

<a id="judge-parameters"></a>
### Judge

`Model Testing › Judge`

Sets judge temperature; the adjacent unlabeled choice uses the shared [Thinking](#thinking) options for judges. **Default:** temperature `0.3` and thinking `Auto`.

Catalog: [Thinking on or off, per model](../../features.md#thinking-on-or-off-per-model).

<a id="model-parameters"></a>
### Models

`Model Testing › Models`

Sets the temperature used by models under test. **Default:** `0.3`.

Catalog: [Which model is actually better at my task?](../../features.md#which-model-is-actually-better-at-my-task).

### Thinking

`Model Testing › Thinking`

Chooses `Auto`, `On`, or `Off` reasoning for models under test; the Judge parameter row has its own corresponding selector. **Default:** `Auto`.

Catalog: [Thinking on or off, per model](../../features.md#thinking-on-or-off-per-model).

### Run test

`Model Testing › Run test`

Starts the configured comparison and becomes `Cancel` while work is in flight. **Default:** idle.

Catalog: [See what's happening between dispatch and verdict](../../features.md#see-whats-happening-between-dispatch-and-verdict).

### Copy Markdown

`Model Testing › Copy Markdown`

Copies the generated Markdown artifact after a run. **Default:** hidden until results exist.

Catalog: [One Markdown artifact you can keep](../../features.md#one-markdown-artifact-you-can-keep).

### Save

`Model Testing › Save`

Writes the artifact to the last chosen path. It combines with [Save as…](#save-as). **Default:** unavailable until a path and results exist.

Catalog: [One Markdown artifact you can keep](../../features.md#one-markdown-artifact-you-can-keep).

### Save as…

`Model Testing › Save as…`

Chooses a path and writes the current Markdown artifact. **Default:** no path selected.

Catalog: [One Markdown artifact you can keep](../../features.md#one-markdown-artifact-you-can-keep).
