/** Dispatched on window to run the setup again (More › App › Setup guide). */
export const RUN_SETUP_EVENT = "handy:run-setup";

export const runSetupAgain = () =>
  window.dispatchEvent(new Event(RUN_SETUP_EVENT));
