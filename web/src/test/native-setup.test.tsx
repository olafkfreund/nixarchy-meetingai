import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { NativeSetup } from "@/routes/downloads";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Native setup platform selection", () => {
  it.each([
    ["Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "Windows", "PowerShell"],
    ["Mozilla/5.0 (X11; Linux x86_64)", "Linux", "terminal"],
  ])("selects the detected desktop for %s", (userAgent, platform, shell) => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue(userAgent);
    render(<NativeSetup />);

    expect(screen.getByRole("button", { name: platform })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    expect(screen.getByText(`One-time setup · ${shell}`)).toBeInTheDocument();
    expect(screen.getByText("lenny", { selector: "code" })).toBeInTheDocument();
  });

  it("lets the user override detection and copies the command for that choice", async () => {
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Linux x86_64");
    const writeText = vi.fn().mockResolvedValue(null);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(<NativeSetup />);
    fireEvent.click(screen.getByRole("button", { name: "Windows" }));
    expect(screen.getByRole("button", { name: "Windows" })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    const copy = screen.getAllByRole("button", { name: "Copy" }).at(0);
    if (!copy) {
      throw new Error("The setup command has no copy button");
    }
    fireEvent.click(copy);
    expect(writeText).toHaveBeenCalledWith(
      "irm https://lennette.itssingularity.com/install.ps1 | iex"
    );
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });
});
