import "@testing-library/jest-dom/vitest";

const noop = () => {
  // jsdom stubs: these exist only so components calling the browser APIs find them.
};

Object.defineProperty(window, "scrollTo", {
  writable: true,
  value: noop,
});

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: noop,
    removeListener: noop,
    addEventListener: noop,
    removeEventListener: noop,
    dispatchEvent: () => false,
  }),
});
