import { useCallback, useEffect, useId, useRef, useState } from "react";

import { joinWaitlist } from "@/lib/waitlist";

type Status =
  | { kind: "idle" }
  | { kind: "sending" }
  | { kind: "done"; message: string }
  | { kind: "error"; message: string };

export const WaitlistDialog = ({
  open,
  onClose,
}: {
  open: boolean;
  onClose: () => void;
}) => {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const titleId = useId();

  // Keep the form mounted while closing so the reset is not visible mid-fade.
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) {
      return;
    }
    if (open && !dialog.open) {
      dialog.showModal();
      inputRef.current?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  }, [open]);

  // The native close event is the single source of truth, so Escape, the
  // close button and backdrop dismissal all reset the same way.
  const handleNativeClose = useCallback(() => {
    setStatus({ kind: "idle" });
    setEmail("");
    onClose();
  }, [onClose]);

  const dismiss = useCallback(() => {
    dialogRef.current?.close();
  }, []);

  const submit = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      const value = email.trim();
      if (!value || status.kind === "sending") {
        return;
      }
      setStatus({ kind: "sending" });
      try {
        const result = await joinWaitlist({
          data: { email: value, userAgent: navigator.userAgent },
        });
        setStatus(
          result.ok
            ? { kind: "done", message: result.message }
            : { kind: "error", message: result.message }
        );
      } catch (error) {
        setStatus({
          kind: "error",
          message:
            error instanceof Error
              ? error.message
              : "Something went wrong. Try again in a moment.",
        });
      }
    },
    [email, status.kind]
  );

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault();
        dismiss();
      }}
      onClose={handleNativeClose}
      className="glass m-auto w-[calc(100%-2rem)] max-w-md rounded-3xl p-7 text-ink backdrop:bg-ink/25 backdrop:backdrop-blur-sm sm:p-8"
    >
      <div className="relative z-10">
        <div className="flex items-start justify-between gap-4">
          <h2 id={titleId} className="font-display text-2xl leading-tight">
            {status.kind === "done"
              ? "You are on the list."
              : "Join the waitlist"}
          </h2>
          <button
            type="button"
            onClick={dismiss}
            aria-label="Close"
            className="-mt-1 rounded-full p-1.5 text-ink-soft transition-colors hover:bg-white/50 hover:text-ink"
          >
            <svg
              viewBox="0 0 24 24"
              aria-hidden="true"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              className="size-4"
            >
              <path d="m6 6 12 12M18 6 6 18" />
            </svg>
          </button>
        </div>

        {status.kind === "done" ? (
          <p className="mt-3 text-sm leading-relaxed text-ink-soft">
            {status.message} A note is on its way to{" "}
            <span className="text-ink">{email.trim()}</span>. It covers what you
            get and when the plan opens.
          </p>
        ) : (
          <>
            <p className="mt-3 text-sm leading-relaxed text-ink-soft">
              Managed is not open yet. Leave your email and you will get the
              first invite, plus a note explaining exactly what it includes.
            </p>

            <form onSubmit={submit} className="mt-6">
              <label
                htmlFor="waitlist-email"
                className="text-xs font-medium text-ink-soft"
              >
                Email address
              </label>
              <input
                ref={inputRef}
                id="waitlist-email"
                type="email"
                required
                autoComplete="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                placeholder="you@example.com"
                disabled={status.kind === "sending"}
                className="glass mt-2 block w-full rounded-full border border-white/70 px-5 py-3 text-sm text-ink placeholder:text-ink-soft/60 focus-visible:border-sky-deep/50 focus-visible:outline-none"
              />

              <button
                type="submit"
                disabled={status.kind === "sending"}
                className="glass btn-liquid mt-4 w-full glass-pill glass-blue text-on-sky disabled:opacity-70"
              >
                <span>
                  {status.kind === "sending" ? "Joining…" : "Join the waitlist"}
                </span>
              </button>
            </form>

            {status.kind === "error" ? (
              <p
                role="alert"
                className="mt-3 rounded-xl border border-ink/15 bg-white/60 px-3 py-2 text-xs leading-relaxed text-ink-soft"
              >
                {status.message}
              </p>
            ) : null}

            <p className="mt-4 text-xs leading-relaxed text-ink-soft/80">
              One email about the launch. No newsletter, no drip sequence.
            </p>
          </>
        )}
      </div>
    </dialog>
  );
};
