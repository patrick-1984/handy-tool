import React from "react";

import ModelSelector from "../model-selector";

/** Under the pages: the model picker, and the model's state at the far end. */
const Footer: React.FC = () => (
  <div className="w-full shrink-0 border-t border-border bg-background">
    <div className="flex items-center gap-4 h-11 px-6 text-xs">
      <ModelSelector />
    </div>
  </div>
);

export default Footer;
