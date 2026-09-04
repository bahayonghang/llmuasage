import { invoke } from "@tauri-apps/api/core";

const root = document.getElementById("root");

async function showRuntimeInfo() {
  const info = await invoke("runtime_info");
  console.log(info);
  if (root) {
    root.textContent = JSON.stringify(info, null, 2);
  }
}

document.addEventListener("DOMContentLoaded", () => {
  const button = document.getElementById("runtime-info");
  button?.addEventListener("click", () => {
    void showRuntimeInfo();
  });
});
