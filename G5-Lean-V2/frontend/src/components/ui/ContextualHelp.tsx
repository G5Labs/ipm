import { CircleHelp } from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";

/** Keyboard-accessible, opt-in help for short explanations beside a control. */
export function ContextualHelp({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      if (
        ref.current &&
        event.target instanceof Node &&
        !ref.current.contains(event.target)
      )
        ref.current.open = false;
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, []);
  return (
    <details ref={ref} className="relative text-sm font-normal">
      <summary
        tabIndex={0}
        onKeyDown={(event) => {
          if (event.key === "Escape" && ref.current?.open) {
            ref.current.open = false;
            ref.current.querySelector("summary")?.focus();
            event.stopPropagation();
          }
        }}
        aria-label={label}
        title={label}
        className="flex cursor-pointer list-none items-center rounded p-1 text-[var(--g5-text2)] hover:text-[var(--g5-text0)] focus-visible:outline focus-visible:outline-2 [&::-webkit-details-marker]:hidden"
      >
        <CircleHelp size={16} aria-hidden="true" />
      </summary>
      <div className="absolute right-0 top-full z-20 mt-1 w-72 max-w-[75vw] space-y-2 rounded-lg border border-[var(--g5-stroke)] bg-[var(--g5-bg0)] p-3 text-xs leading-relaxed text-[var(--g5-text1)] shadow-lg">
        {children}
      </div>
    </details>
  );
}
