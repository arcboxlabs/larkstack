import { login } from "./lib/auth";

export function Login() {
  return (
    <main className="login">
      <h1>larkstack console</h1>
      <p className="muted">Sign in with your Lark account to continue.</p>
      <button type="button" onClick={login}>
        Sign in with Lark
      </button>
    </main>
  );
}
