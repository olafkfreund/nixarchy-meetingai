import { useCallback, useState } from "react";

const copyText = async (text: string): Promise<boolean> => {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Clipboard access needs a secure context and can be blocked by policy.
    return false;
  }
};

export const Command = ({
  children,
  label,
}: {
  children: string;
  label?: string;
}) => {
  const [copied, setCopied] = useState(false);

  const onCopy = useCallback(async () => {
    const ok = await copyText(children);
    if (!ok) {
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 1600);
  }, [children]);

  return (
    <div className="group relative overflow-hidden rounded-2xl bg-ink text-[0.92rem] shadow-[0_1px_2px_oklch(0.3_0.05_250/0.18)]">
      <div className="flex items-center justify-between gap-4 border-b border-white/10 px-4 py-2">
        <span className="text-xs font-medium tracking-wide text-white/50">
          {label ?? "shell"}
        </span>
        <button
          type="button"
          onClick={onCopy}
          className="rounded-lg px-2 py-1 text-xs font-medium text-white/60 transition-colors hover:bg-white/10 hover:text-white"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="overflow-x-auto px-4 py-3.5 leading-relaxed text-white/95">
        <code>{children}</code>
      </pre>
    </div>
  );
};

export const CommandList = ({ commands }: { commands: string[] }) => (
  <div className="flex flex-col gap-3">
    {commands.map((command) => (
      <Command key={command}>{command}</Command>
    ))}
  </div>
);
