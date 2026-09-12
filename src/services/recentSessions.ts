import { getActiveNodeId } from "./nodeConfig";

export interface RecentSession {
  source: string;
  projectId: string;
  filePath: string;
  title: string;
}

export const RECENT_SESSIONS_CHANGED = "asv-recent-sessions-changed";
const storageKey = () => `asv-recent-sessions:${getActiveNodeId()}`;

export function readRecentSessions(): RecentSession[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(storageKey()) || "[]");
    return Array.isArray(value) ? value.filter((item): item is RecentSession =>
      item && [item.source, item.projectId, item.filePath, item.title].every((field) => typeof field === "string")
    ).slice(0, 20) : [];
  } catch { return []; }
}

export function rememberSession(session: RecentSession): void {
  try {
    const previous = readRecentSessions();
    if (JSON.stringify(previous[0]) === JSON.stringify(session)) return;
    const next = [session, ...previous.filter((item) => item.source !== session.source || item.filePath !== session.filePath)].slice(0, 20);
    localStorage.setItem(storageKey(), JSON.stringify(next));
    window.dispatchEvent(new Event(RECENT_SESSIONS_CHANGED));
  } catch { /* A disabled preference store must not prevent reading sessions. */ }
}
