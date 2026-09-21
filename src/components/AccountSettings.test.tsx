// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAccount, type AccountView } from "../hooks/useAccount";
import { AccountSettings } from "./AccountSettings";
const { invoke } = vi.hoisted(() => ({invoke: vi.fn()}));
vi.mock("@tauri-apps/api/core", () => ({invoke}));
const signedOut: AccountView = {status:"signed_out", configured:true, profile:null, message:"Sign in"};
const signedIn: AccountView = {status:"signed_in", configured:true, profile:{name:"EmberTester",id:"0123456789abcdef0123456789abcdef"},message:"Java Edition verified"};
function Harness() { const auth = useAccount(); return <AccountSettings auth={auth} />; }
beforeEach(() => { invoke.mockImplementation(async (command:string) => command === "auth_sign_in" ? signedIn : signedOut); });
afterEach(() => { cleanup(); invoke.mockReset(); });
describe("Microsoft account UI", () => {
  it("signs in and shows only the public Minecraft identity, then signs out", async () => {
    const user = userEvent.setup(); render(<Harness />);
    await screen.findByText("Sign in");
    await user.click(screen.getByRole("button", {name:"Sign in with Microsoft"}));
    expect(await screen.findByText("EmberTester")).toBeTruthy();
    expect(screen.getByText(signedIn.profile!.id)).toBeTruthy();
    await user.click(screen.getByRole("button", {name:"Sign out"}));
    await waitFor(() => expect(screen.queryByText("EmberTester")).toBeNull());
    expect(invoke).toHaveBeenCalledWith("auth_sign_out");
  });
  it("restores a saved account on startup", async () => {
    invoke.mockResolvedValue(signedIn); render(<Harness />);
    expect(await screen.findByText("EmberTester")).toBeTruthy();
    expect(invoke).toHaveBeenCalledWith("auth_restore");
  });
  it("explains missing configuration and disables sign-in", async () => {
    invoke.mockResolvedValue({...signedOut, configured:false, message:"Configure your client ID"}); render(<Harness />);
    await screen.findByText("Configure your client ID");
    expect((screen.getByRole("button", {name:"Sign in with Microsoft"}) as HTMLButtonElement).disabled).toBe(true);
  });
  it("supports cancellation without duplicate sign-ins", async () => {
    let finish: (value:AccountView)=>void = () => {};
    invoke.mockImplementation(async (command:string) => {
      if (command === "auth_sign_in") return new Promise<AccountView>(resolve => {finish = resolve;});
      if (command === "auth_cancel") finish({...signedOut,message:"Sign-in cancelled"});
      return signedOut;
    });
    const user = userEvent.setup(); render(<Harness />); await screen.findByText("Sign in");
    await user.dblClick(screen.getByRole("button",{name:"Sign in with Microsoft"}));
    await user.click(screen.getByRole("button",{name:"Cancel sign-in"}));
    await screen.findByText("Sign-in cancelled");
    expect(invoke.mock.calls.filter(([command]) => command === "auth_sign_in")).toHaveLength(1);
  });
  it("shows sanitized refresh errors and lets the user retry", async () => {
    invoke.mockResolvedValue({...signedOut,status:"error",message:"Check your connection"});
    const user = userEvent.setup(); render(<Harness />);
    expect(await screen.findByRole("alert")).toHaveProperty("textContent","Check your connection");
    invoke.mockResolvedValue(signedIn);
    await user.click(screen.getByRole("button",{name:"Retry saved sign-in"}));
    await screen.findByText("EmberTester");
  });
});
