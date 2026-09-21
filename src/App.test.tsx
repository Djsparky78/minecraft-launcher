// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class { onmessage = () => {}; } }));
const installed = {version:"1.21", status:"installed", location:"C:\\Ember\\game\\versions\\1.21", error:null};
let list: typeof installed[];
let versionError = false;
let waitInstall: (() => void) | null;
beforeEach(() => {
  list = []; versionError = false; waitInstall = null;
  invoke.mockImplementation(async (command: string, args?: {version:string; onProgress?: {onmessage:(event:unknown)=>void}}) => {
    if (command === "minecraft_versions") {
      if (versionError) throw "Network unavailable";
      return {latest_release:"1.21",versions:[{id:"1.21",url:"a"},{id:"1.20.1",url:"b"}],cached:false,warning:null};
    }
    if (command === "auth_restore") return {status:"signed_out", configured:false, profile:null, message:"Configure Microsoft sign-in"};
    if (command === "detect_java") return [{path:"C:\\Java\\bin\\java.exe",version:"21.0.4",major_version:21,vendor:"Eclipse Adoptium"}];
    if (command === "installed_versions") return [...list];
    if (command === "install_version") {
      args?.onProgress?.onmessage({version:args.version,stage:"Assets",file:"example.ogg",percent:50,completed_files:2,total_files:4,file_bytes:5,file_total:10});
      if (waitInstall) await new Promise<void>(resolve => { waitInstall = resolve; });
      list = [{...installed,version:args!.version}];
    }
  });
});
afterEach(() => {cleanup();invoke.mockReset();});
describe("launcher installation", () => {
  it("preserves version selection and Java Settings", async () => {
    const user=userEvent.setup();render(<App />);
    await screen.findByRole("option",{name:"1.20.1"});
    await user.selectOptions(screen.getByRole("combobox"),"1.20.1");
    await user.click(screen.getByRole("button",{name:"Settings"}));
    expect(await screen.findByText("Java 21")).toBeTruthy();
    expect(screen.getByText(/Eclipse Adoptium/)).toBeTruthy();
    await user.click(screen.getByRole("button",{name:"Play"}));
    expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("1.20.1");
  });
  it("installs selected version, renders progress, then marks it ready", async () => {
    const user=userEvent.setup();render(<App />);
    await screen.findByRole("option",{name:"1.21 (Latest release)"});
    await user.click(screen.getByRole("button",{name:"INSTALL"}));
    expect(invoke).toHaveBeenCalledWith("install_version",expect.objectContaining({version:"1.21"}));
    expect(await screen.findByRole("button",{name:"PLAY"})).toBeTruthy();
    expect(screen.getByRole("progressbar").getAttribute("value")).toBe("50");
    expect(screen.getByText(/installed and ready. Game launching/)).toBeTruthy();
    await user.click(screen.getByRole("button",{name:"Installations"}));
    expect(screen.getByText(installed.location)).toBeTruthy();
  });
  it("does not download or launch an already installed version", async () => {
    list=[installed];const user=userEvent.setup();render(<App />);
    await user.click(await screen.findByRole("button",{name:"PLAY"}));
    expect(await screen.findByText(/is installed and ready/)).toBeTruthy();
    expect(invoke.mock.calls.some(([command])=>command === "install_version" || command === "launcher_status")).toBe(false);
  });
  it("repairs an installed version from Installations", async () => {
    list=[installed];const user=userEvent.setup();render(<App />);
    await screen.findByRole("button",{name:"PLAY"});
    await user.click(screen.getByRole("button",{name:"Installations"}));
    await user.click(screen.getByRole("button",{name:"Repair / reverify 1.21"}));
    await waitFor(()=>expect(invoke).toHaveBeenCalledWith("install_version",expect.objectContaining({version:"1.21"})));
  });
  it("blocks duplicate clicks and sends cancellation", async () => {
    waitInstall=()=>{}; const user=userEvent.setup();render(<App />);
    await screen.findByRole("option",{name:"1.21 (Latest release)"});
    await user.dblClick(screen.getByRole("button",{name:"INSTALL"}));
    await user.click(await screen.findByRole("button",{name:"Cancel installation"}));
    expect(invoke).toHaveBeenCalledWith("cancel_install",{version:"1.21"});
    expect(invoke.mock.calls.filter(([command])=>command === "install_version")).toHaveLength(1);
    waitInstall!();
    await screen.findByRole("button",{name:"PLAY"});
  });
  it("shows network errors and permits retry", async () => {
    versionError=true; const user=userEvent.setup();render(<App />);
    expect((screen.getByRole("button",{name:"INSTALL"}) as HTMLButtonElement).disabled).toBe(true);
    await screen.findByText("Network unavailable"); versionError=false;
    await user.click(screen.getByRole("button",{name:"Refresh versions"}));
    await screen.findByRole("option",{name:"1.21 (Latest release)"});
    expect((screen.getByRole("button",{name:"INSTALL"}) as HTMLButtonElement).disabled).toBe(false);
  });
  it("reports installer errors and allows a new attempt", async () => {
    const base=invoke.getMockImplementation()!;
    invoke.mockImplementation(async (command:string,...args:unknown[])=> {
      if(command === "install_version") throw "Disk is full";
      return base(command,...args);
    });
    const user=userEvent.setup();render(<App />);await screen.findByRole("option",{name:"1.21 (Latest release)"});
    await user.click(screen.getByRole("button",{name:"INSTALL"}));
    expect(await screen.findByRole("alert")).toHaveProperty("textContent","Disk is full");
    await waitFor(()=>expect((screen.getByRole("button",{name:"INSTALL"}) as HTMLButtonElement).disabled).toBe(false));
  });
});
