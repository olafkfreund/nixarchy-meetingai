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
    <div className="glass relative overflow-hidden rounded-2xl">
      <div className="relative z-10 flex items-center justify-between gap-4 border-b border-ink/10 bg-white/25 px-4 py-2">
        <span className="text-xs font-medium text-ink-soft">
          {label ?? "shell"}
        </span>
        <button
          type="button"
          onClick={onCopy}
          className="rounded-lg px-2 py-1 text-xs font-medium text-sky-deep transition-colors hover:bg-sky-deep/10 hover:text-ink"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="relative z-10 overflow-x-auto bg-white/35 px-4 py-3.5 text-[0.92rem] leading-relaxed text-ink">
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
