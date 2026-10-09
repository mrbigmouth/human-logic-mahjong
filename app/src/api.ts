// 呼叫核心的唯一入口：桌面版（Tauri）走 command `api`，網頁版（`mj serve`）走 POST /api/<名稱>。
// 兩邊在 Rust 端都轉給 mj_app::App::call，所以名稱與參數完全相同。

interface TauriInternals {
  invoke: (cmd: string, args: unknown) => Promise<unknown>;
}

function tauri(): TauriInternals | undefined {
  return (window as unknown as { __TAURI_INTERNALS__?: TauriInternals }).__TAURI_INTERNALS__;
}

export async function call<T>(name: string, args: unknown = null): Promise<T> {
  const t = tauri();
  if (t) {
    return (await t.invoke("api", { name, args })) as T;
  }
  const res = await fetch(`/api/${name}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args),
  });
  const body = (await res.json()) as { ok?: T; error?: string };
  if (body.error !== undefined) {
    throw new Error(body.error);
  }
  return body.ok as T;
}
