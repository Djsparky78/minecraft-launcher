// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

afterEach(() => {
  cleanup();
  invoke.mockReset();
});

describe("launcher foundation", () => {
  it("lets the player select a Minecraft version", async () => {
    const user = userEvent.setup();
    render(<App />);

    const selector = screen.getByRole("combobox", { name: "VERSION" });
    await user.selectOptions(selector, "1.20.1 Forge");

    expect((selector as HTMLSelectElement).value).toBe("1.20.1 Forge");
  });

  it("invokes the Rust launcher status command from the Play button", async () => {
    const user = userEvent.setup();
    invoke.mockResolvedValue("Launcher services ready");
    render(<App />);

    await user.click(screen.getByRole("button", { name: "PLAY" }));

    expect(invoke).toHaveBeenCalledOnce();
    expect(invoke).toHaveBeenCalledWith("launcher_status");
    expect(await screen.findByText("Launcher services ready")).toBeTruthy();
  });
});
