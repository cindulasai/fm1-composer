// Stand-in for @tauri-apps/api/event in the browser: text2midi's window listens to "compose-progress".
const bus = new EventTarget();
export function emit(name, payload) { bus.dispatchEvent(new CustomEvent(name, { detail: payload })); }
export async function listen(name, handler) {
  const f = (e) => handler({ event: name, payload: e.detail });
  bus.addEventListener(name, f);
  return () => bus.removeEventListener(name, f);
}
