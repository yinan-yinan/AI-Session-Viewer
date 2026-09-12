import { useEffect, useRef, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

/** A disclosure for secondary actions; keeps native keyboard activation. */
export function ActionMenu({ label, children }: { label: string; children: ReactNode }) {
  const ref = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const dismiss = (event: PointerEvent) => {
      if (event.target instanceof Node && !ref.current?.contains(event.target) && ref.current) {
        ref.current.open = false;
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && ref.current?.open) {
        ref.current.open = false;
        ref.current.querySelector("summary")?.focus();
      }
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", escape);
    };
  }, []);
  return (
    <details ref={ref} className="action-menu">
      <summary className="toolbar-button cursor-pointer list-none">
        {label}<ChevronDown className="h-3.5 w-3.5" />
      </summary>
      <div className="action-menu-panel" onClick={(event) => {
        if (event.target instanceof Element && event.target.closest("button") && ref.current) {
          ref.current.open = false;
          ref.current.querySelector("summary")?.focus();
        }
      }}>{children}</div>
    </details>
  );
}
