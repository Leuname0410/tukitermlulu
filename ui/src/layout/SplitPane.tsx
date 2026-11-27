import type { JSX, ReactNode } from "react";

interface SplitPaneProps {
  sessionId?: number;
  left: ReactNode;
  right: ReactNode;
}

export function SplitPane({ left, right }: SplitPaneProps): JSX.Element {
  return (
    <div>
      {left}
      {right}
    </div>
  );
}
