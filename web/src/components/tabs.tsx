import { useCallback, useState } from "react";

export const Tabs = ({
  tabs,
  children,
}: {
  tabs: { id: string; label: string }[];
  children: (activeId: string) => React.ReactNode;
}) => {
  const [active, setActive] = useState(tabs[0]?.id ?? "");

  const select = useCallback((id: string) => {
    setActive(id);
  }, []);

  return (
    <div>
      <div
        role="tablist"
        aria-label="Installation method"
        className="glass mb-8 inline-flex gap-1 glass-pill p-1"
      >
        {tabs.map((tab) => {
          const selected = tab.id === active;
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={selected}
              aria-controls={`panel-${tab.id}`}
              id={`tab-${tab.id}`}
              onClick={() => select(tab.id)}
              className={`rounded-full px-5 py-2.5 text-sm font-medium transition-colors ${
                selected
                  ? "bg-white/80 text-ink shadow-[0_1px_2px_oklch(0.4_0.08_250/0.12)]"
                  : "text-ink-soft hover:text-ink"
              }`}
            >
              {tab.label}
            </button>
          );
        })}
      </div>
      {tabs.map((tab) => (
        <div
          key={tab.id}
          role="tabpanel"
          id={`panel-${tab.id}`}
          aria-labelledby={`tab-${tab.id}`}
          hidden={tab.id !== active}
        >
          {children(tab.id)}
        </div>
      ))}
    </div>
  );
};
