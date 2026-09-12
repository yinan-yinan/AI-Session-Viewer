import type { ForkResult } from "./tauriApi";

interface ForkApi {
  forkSession(source: string, filePath: string, messageId: string): Promise<ForkResult>;
  resumeSession(source: string, sessionId: string, projectPath: string, filePath?: string, shell?: string): Promise<void>;
}

/** A terminal failure must never turn an already-created fork into a retry. */
export async function createSessionFork(
  api: ForkApi,
  source: string,
  filePath: string,
  messageId: string,
  openTerminal: boolean,
  shell?: string,
): Promise<{ result: ForkResult; warning: string | null }> {
  const result = await api.forkSession(source, filePath, messageId);
  let warning: string | null = null;
  if (openTerminal) {
    try {
      await api.resumeSession(source, result.newSessionId, result.projectPath, result.newFilePath, shell);
    } catch (error) {
      warning = `新会话已创建，但终端打开失败：${error instanceof Error ? error.message : String(error)}。可使用恢复按钮重试。`;
    }
  }
  return { result, warning };
}
