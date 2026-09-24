export interface OmpModelConfig {
  id: string;
  name?: string | null;
}

export interface OmpProviderConfig {
  id: string;
  baseUrl: string;
  apiKey: string;
  api: string;
  userAgent: string;
  discoveryType: string;
  models: OmpModelConfig[];
  enabled: boolean;
}

export interface OmpDiscoveredModel {
  provider: string;
  id: string;
  name: string;
}

export const OMP_API_FORMATS = [
  "openai-completions",
  "openai-responses",
  "openai-codex-responses",
  "azure-openai-responses",
  "anthropic-messages",
  "google-generative-ai",
  "google-gemini-cli",
  "google-vertex",
] as const;

export const OMP_DISCOVERY_TYPES = [
  "ollama",
  "llama.cpp",
  "lm-studio",
  "openai-models-list",
  "proxy",
  "litellm",
] as const;
