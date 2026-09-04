import { invoke } from "@tauri-apps/api/core";

let nextRequestId = 1;

export function allocateRequestId(): number {
  const id = nextRequestId;
  nextRequestId += 1;
  return id;
}

export function resetRequestIdCounter(value = 1): void {
  nextRequestId = value;
}

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args);
}

export type DesktopCommandError = {
  code: string;
  message: string;
  holder?: string;
};

export function normalizeInvokeError(error: unknown): DesktopCommandError {
  if (typeof error === "string") {
    return parseErrorText(error);
  }
  if (error && typeof error === "object") {
    const record = error as Record<string, unknown>;
    if (typeof record.code === "string") {
      return {
        code: record.code,
        message: typeof record.message === "string" ? record.message : record.code,
        ...(typeof record.holder === "string" ? { holder: record.holder } : {}),
      };
    }
    if ("error" in record && record.error !== error) {
      return normalizeInvokeError(record.error);
    }
    if (typeof record.message === "string") {
      return parseErrorText(record.message);
    }
  }
  return parseErrorText(String(error ?? "error"));
}

function parseErrorText(text: string): DesktopCommandError {
  const match = text.match(/^([a-z_]+):\s*(.*)$/);
  if (!match) {
    return { code: "error", message: text };
  }
  const message = match[2] || text;
  const holderMatch = message.match(/^(.*) \(([^)]+)\)\s*$/);
  if (holderMatch) {
    return { code: match[1], message: holderMatch[1], holder: holderMatch[2] };
  }
  return { code: match[1], message };
}
