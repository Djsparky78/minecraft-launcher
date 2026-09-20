// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => invoke.mockImplementation(async (command: string) => {
  if (command === "minecraft_versions") return { versions: [{id: "1.21", url: "a"}, {id:"1.20.1", url:"b"}], cached:false, warning:null };
  if (command === "detect_java") return [{path:"C:\\Java\\bin\\java.exe", version:"21.0.4"}];
  return "Launcher services ready";
}));
afterEach(() => { cleanup(); invoke.mockReset(); });
describe("launcher environment", () => {
  it("loads releases, changes selection, displays Java and invokes status", async () => {
    const user = userEvent.setup(); render(<App />);
    await screen.findByRole("option", {name:"1.20.1"});
    await user.selectOptions(screen.getByRole("combobox"), "1.20.1");
    expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("1.20.1");
    expect(await screen.findByText("Java 21.0.4")).toBeTruthy();
    await user.click(screen.getByRole("button", {name:"PLAY"}));
    expect(invoke).toHaveBeenCalledWith("launcher_status");
    expect(await screen.findByText("Launcher services ready")).toBeTruthy();
  });
  it("shows cached data and no Java state", async () => {
    invoke.mockImplementation(async (command: string) => command === "minecraft_versions" ? {versions:[{id:"1.21",url:"a"}],cached:true,warning:"Showing saved versions"} : []);
    render(<App />);
    expect(await screen.findByText("Showing saved versions")).toBeTruthy();
    expect(await screen.findByText(/No Java found/)).toBeTruthy();
  });
  it("disables Play while loading and recovers from an error via refresh", async () => {
    invoke.mockRejectedValue("Network unavailable"); render(<App />);
    expect((screen.getByRole("button", {name:"PLAY"}) as HTMLButtonElement).disabled).toBe(true);
    await waitFor(() => expect(screen.getAllByText("Network unavailable").length).toBeGreaterThan(0));
    invoke.mockResolvedValue({versions:[{id:"1.21",url:"a"}],cached:false,warning:null});
    await userEvent.click(screen.getByRole("button", {name:"Refresh versions"}));
    await screen.findByRole("option", {name:"1.21"});
    expect((screen.getByRole("button", {name:"PLAY"}) as HTMLButtonElement).disabled).toBe(false);
  });
});
