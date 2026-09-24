import { useEffect, useState } from "react";
import { AlertTriangle, ArrowLeft, Eye, EyeOff, Loader2, Pencil, Plus, RefreshCw, Save, Trash2, X } from "lucide-react";
import { api } from "../../services/api";
import type { OmpDiscoveredModel, OmpProviderConfig } from "../../types/ompModels";
import { OMP_API_FORMATS, OMP_DISCOVERY_TYPES } from "../../types/ompModels";
import { useAppStore } from "../../stores/appStore";

type Editor = { originalId: string | null; config: OmpProviderConfig };
type ConfirmDialog = { kind: "disable" | "delete"; provider: OmpProviderConfig } | null;
const emptyProvider = (): OmpProviderConfig => ({
  id: "", baseUrl: "", apiKey: "", api: "anthropic-messages", userAgent: "", discoveryType: "openai-models-list", models: [], enabled: true,
});
const API_URL_PLACEHOLDERS: Record<string, string> = {
  "openai-completions": "https://api.example.com/v1",
  "openai-responses": "https://api.example.com/v1",
  "openai-codex-responses": "https://api.example.com/v1",
  "azure-openai-responses": "https://{resource}.openai.azure.com/openai/v1",
  "anthropic-messages": "https://api.anthropic.com",
  "google-generative-ai": "按服务商文档填写 API 地址",
  "google-gemini-cli": "按服务商文档填写 API 地址",
  "google-vertex": "按服务商文档填写区域 API 地址",
};
const fieldClass = "mt-1 w-full min-w-0 rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary";
function ConfirmModal({
  title,
  description,
  confirmLabel,
  destructive = false,
  busy = false,
  onConfirm,
  onCancel,
}: {
  title: string;
  description: string;
  confirmLabel: string;
  destructive?: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onCancel(); }}>
      <div className="w-full max-w-md rounded-xl border border-border bg-card p-5 shadow-xl" role="dialog" aria-modal="true" aria-labelledby="omp-confirm-title">
        <div className="flex items-start gap-3">
          <AlertTriangle className={`mt-0.5 h-5 w-5 shrink-0 ${destructive ? "text-destructive" : "text-amber-500"}`} />
          <div className="min-w-0">
            <h2 id="omp-confirm-title" className="font-semibold">{title}</h2>
            <p className="mt-2 whitespace-pre-line break-words text-sm leading-6 text-muted-foreground">{description}</p>
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <button type="button" onClick={onCancel} disabled={busy} className="rounded-lg border border-border px-3 py-2 text-sm hover:bg-accent disabled:opacity-50">取消</button>
          <button type="button" onClick={onConfirm} disabled={busy} className={`inline-flex items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium text-white disabled:opacity-50 ${destructive ? "bg-destructive hover:bg-destructive/90" : "bg-primary hover:bg-primary/90"}`}>
            {busy && <Loader2 className="h-4 w-4 animate-spin" />}{busy ? "处理中…" : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

function animateOmpBackdrop(node: HTMLDivElement | null) {
  if (node && !window.matchMedia("(prefers-reduced-motion: reduce)").matches)
    node.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 200, easing: "ease-out" });
}
function animateOmpDrawer(node: HTMLElement | null) {
  if (node)
    node.animate([{ transform: "translateX(100%)" }, { transform: "translateX(0)" }], { duration: 300, easing: "cubic-bezier(0.4, 0, 0.2, 1)" });
}

export function OmpModelsPage() {
  const source = useAppStore((state) => state.source);
  const ompModelsByProvider = useAppStore((state) => state.ompModelsByProvider);
  const ompModelsLoaded = useAppStore((state) => state.ompModelsLoaded);
  const ompModelsLoading = useAppStore((state) => state.ompModelsLoading);
  const ompModelsError = useAppStore((state) => state.ompModelsError);
  const refreshOmpModels = useAppStore((state) => state.refreshOmpModels);
  const invalidateOmpModels = useAppStore((state) => state.invalidateOmpModels);
  const [confirmDialog, setConfirmDialog] = useState<ConfirmDialog>(null);
  const [confirmBusy, setConfirmBusy] = useState(false);
  const [providers, setProviders] = useState<OmpProviderConfig[]>([]);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [showApiKey, setShowApiKey] = useState(false);
  const [showModels, setShowModels] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [fetching, setFetching] = useState(false);
  const [refreshElapsed, setRefreshElapsed] = useState(0);
  const [preview, setPreview] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(""), 3000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    if (!fetching) { setRefreshElapsed(0); return; }
    const started = Date.now();
    const timer = window.setInterval(() => setRefreshElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => window.clearInterval(timer);
  }, [fetching]);

  const loadProviders = async (): Promise<OmpProviderConfig[]> => {
    setLoading(true);
    try {
      const result = await api.ompListModelProviders();
      setProviders(result);
      return result;
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      return [];
    } finally { setLoading(false); }
  };

  useEffect(() => {
    if (source !== "omp") return;
    void loadProviders();
  }, [source]);

  const detectAllModels = async () => {
    setFetching(true); setError(""); setNotice("");
    try {
      const result = await refreshOmpModels(undefined, true);
      const count = Object.values(result ?? {}).reduce((total, models) => total + models.length, 0);
      setNotice(`检测完成，共发现 ${count} 个模型。`);
    } catch (reason) {
      setError(`模型检测失败，请检查 API 地址、API Key 和 User-Agent。${reason instanceof Error ? ` ${reason.message}` : ""}`);
    } finally { setFetching(false); }
  };

  useEffect(() => {
    if (ompModelsError) setError(`模型列表获取失败，请检查 API 地址、API Key 和 User-Agent。${ompModelsError}`);
  }, [ompModelsError]);
  useEffect(() => {
    if (!editor || !editor.config.id.trim() || !editor.config.baseUrl.trim() || !editor.config.apiKey.trim()) {
      setPreview("");
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(async () => {
      try {
        const text = await api.ompPreviewModelProvider(editor.config, editor.originalId);
        if (!cancelled) setPreview(text);
      } catch { if (!cancelled) setPreview(""); }
    }, 250);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [editor]);

  const beginAdd = () => {
    setEditor({ originalId: null, config: emptyProvider() });
    setShowModels(false); setShowApiKey(false); setError(""); setNotice("");
  };
  const beginEdit = (config: OmpProviderConfig) => {
    setEditor({ originalId: config.id, config: structuredClone(config) });
    setShowModels(false); setShowApiKey(false); setError(""); setNotice("");
  };
  const update = (patch: Partial<OmpProviderConfig>) => setEditor((current) => current ? { ...current, config: { ...current.config, ...patch } } : current);

  const save = async () => {
    if (!editor) return;
    setSaving(true); setError(""); setNotice("");
    try {
      await api.ompSaveModelProvider(editor.config, editor.originalId);
      const saved = { ...editor.config, id: editor.config.id.trim() };
      setEditor({ originalId: saved.id, config: saved });
      await loadProviders();
      invalidateOmpModels(saved.id);
      setNotice(saved.enabled ? "供应商配置已保存。点击“模型列表”后检测模型。" : "未启用供应商配置已保存；点击卡片“启用”后写入 OMP models.yml。");
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setSaving(false); }
  };

  const fetchModels = async () => {
    if (!editor) return;
    if (!editor.config.enabled) { setError("请先启用供应商，再检测模型。"); return; }
    if (editor.originalId !== editor.config.id.trim()) {
      setError("请先保存供应商配置，再查看模型列表。");
      return;
    }
    const providerId = editor.config.id.trim();
    setFetching(true); setError(""); setNotice("");
    try {
      const result = await refreshOmpModels(providerId, true);
      const models = result?.[providerId] ?? [];
      setShowModels(true);
      setNotice(models.length > 0
        ? `已检测到 ${models.length} 个模型。OMP 会自动读取供应商模型，无需手动添加。`
        : "刷新成功，但没有检测到模型。请检查 API 地址、API Key、User-Agent 和模型发现方式。");
    } catch (reason) {
      setError(`模型列表获取失败，请检查 API 地址、API Key 和 User-Agent。${reason instanceof Error ? ` ${reason.message}` : ""}`);
    } finally { setFetching(false); }
  };

  const confirmAction = async () => {
    if (!confirmDialog) return;
    const action = confirmDialog;
    setConfirmBusy(true); setError(""); setNotice("");
    try {
      if (action.kind === "disable") {
        await api.ompDisableModelProvider(action.provider.id);
      } else {
        await api.ompDeleteModelProvider(action.provider.id);
      }
      invalidateOmpModels(action.provider.id);
      await loadProviders();
      setNotice(action.kind === "disable" ? "供应商已禁止；配置仍保留，可重新启用。" : "供应商配置已删除。");
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setConfirmBusy(false); setConfirmDialog(null); }
  };

  const enableProvider = async (provider: OmpProviderConfig) => {
    setError(""); setNotice("");
    try {
      await api.ompEnableModelProvider(provider.id);
      invalidateOmpModels(provider.id);
      await loadProviders();
      setNotice("供应商已启用，配置已写入 OMP models.yml。");
    } catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
  };

  if (source !== "omp") return <div className="workspace-page p-6 text-sm text-muted-foreground">请先切换到 Oh My Pi 数据源。</div>;

  return (
    <>
      <main className="workspace-page h-full min-h-0 overflow-y-auto">
        <div className="mx-auto w-full max-w-6xl space-y-5 p-4 sm:p-6 lg:p-8">
          <header className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <h1 className="text-xl font-semibold">OMP 模型供应商</h1>
              <p className="mt-1 text-sm text-muted-foreground">管理 OMP 供应商配置；启用项写入 models.yml，禁止项保留在列表但不会被 OMP 加载。</p>
            </div>
            <div className="flex flex-wrap gap-2"><button onClick={() => void detectAllModels()} disabled={fetching || loading} className="inline-flex items-center gap-2 rounded-lg border border-border px-4 py-2 text-sm hover:bg-accent disabled:opacity-50"><RefreshCw className={`h-4 w-4 ${fetching ? "animate-spin" : ""}`} />{fetching ? `检测中（${refreshElapsed}s）…` : "检测全部"}</button><button onClick={beginAdd} className="inline-flex shrink-0 items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground"><Plus className="h-4 w-4" />添加供应商</button></div>
          </header>

        {!editor && error && <div role="alert" className="break-words rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">{error}</div>}
        {!editor && notice && <div role="status" className="rounded-lg border border-primary/30 bg-primary/5 p-3 text-sm">{notice}</div>}

        {editor && (
          <>
          <div ref={animateOmpBackdrop} className="fixed inset-0 z-40 bg-black/20 backdrop-blur-[3px]" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) { setEditor(null); setShowModels(false); setError(""); setNotice(""); } }} />
          <section ref={animateOmpDrawer} className="fixed inset-y-0 right-0 z-50 flex w-full max-w-2xl min-w-0 flex-col overflow-y-auto border-l border-border bg-background p-5 shadow-2xl sm:p-8">
            <div className="mb-6 flex items-center gap-3">
              <button type="button" onClick={() => { setEditor(null); setShowModels(false); setError(""); setNotice(""); }} className="inline-flex shrink-0 items-center justify-center rounded-xl border border-border bg-background p-2.5 text-muted-foreground shadow-sm hover:bg-accent" aria-label="返回供应商列表"><ArrowLeft className="h-5 w-5" /></button>
              <h2 className="truncate text-xl font-semibold">{editor.originalId ? "编辑供应商" : "添加新供应商"}</h2>
            </div>
            <div className={`grid min-w-0 grid-cols-1 gap-4 md:grid-cols-2 ${!editor.originalId ? "rounded-xl bg-muted/20 p-4 sm:p-6" : ""}`}>
              <label className="min-w-0 text-sm">供应商名称 / ID<input value={editor.config.id} onChange={(event) => update({ id: event.target.value })} className={fieldClass} placeholder="例如 eflowcode_test" /></label>
              <label className="min-w-0 text-sm">API 格式<select value={editor.config.api} onChange={(event) => update({ api: event.target.value })} className={fieldClass}>{OMP_API_FORMATS.map((value) => <option key={value}>{value}</option>)}</select></label>
              <label className="min-w-0 text-sm md:col-span-2">API 请求地址<input value={editor.config.baseUrl} onChange={(event) => update({ baseUrl: event.target.value })} className={fieldClass} placeholder={API_URL_PLACEHOLDERS[editor.config.api] ?? "https://api.example.com/v1"} /></label>
              <label className="min-w-0 text-sm md:col-span-2">API Key
                <span className="relative mt-1 block min-w-0"><input type={showApiKey ? "text" : "password"} value={editor.config.apiKey} onChange={(event) => update({ apiKey: event.target.value })} className={`${fieldClass} pr-11`} placeholder="环境变量名或实际密钥" autoComplete="off" /><button type="button" onClick={() => setShowApiKey((value) => !value)} className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1.5 text-muted-foreground hover:text-foreground" aria-label={showApiKey ? "隐藏 API Key" : "显示 API Key"}>{showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}</button></span>
                <span className="mt-1 block text-xs text-muted-foreground">OMP 会先按环境变量名解析；未匹配时将输入值作为密钥。YAML 预览按要求明文显示。</span>
              </label>
              <label className="min-w-0 text-sm">自定义 User-Agent<input value={editor.config.userAgent} onChange={(event) => update({ userAgent: event.target.value })} className={fieldClass} placeholder="可选，例如 Mozilla/5.0 ..." /></label>
              <label className="min-w-0 text-sm">模型发现方式<select value={editor.config.discoveryType} onChange={(event) => update({ discoveryType: event.target.value })} className={fieldClass}>{OMP_DISCOVERY_TYPES.map((value) => <option key={value}>{value}</option>)}</select></label>
            </div>

            <div className="mt-5 flex flex-wrap items-center gap-3 border-t border-border pt-5">
              <button onClick={() => void save()} disabled={saving || loading} className="inline-flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground disabled:opacity-50"><Save className="h-4 w-4" />{saving ? "保存中…" : editor.originalId ? "保存配置" : "添加供应商"}</button>
              <button onClick={() => void fetchModels()} disabled={fetching || saving || !editor.config.enabled || !editor.config.id.trim()} className="inline-flex items-center gap-2 rounded-lg border border-border px-4 py-2 text-sm hover:bg-accent disabled:opacity-50"><RefreshCw className={`h-4 w-4 ${fetching ? "animate-spin" : ""}`} />{fetching ? `正在检测（${refreshElapsed}s）…` : "模型列表"}</button>
              {editor.originalId && editor.originalId !== editor.config.id.trim() && <span className="flex min-w-0 items-start gap-1 text-xs text-amber-600"><AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />更改 provider ID 会影响已保存的 provider/model 选择器。</span>}
            </div>
            {editor && error && <div role="alert" className="mt-3 break-words rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">{error}</div>}
            {editor && notice && <div role="status" className="mt-3 rounded-lg border border-primary/30 bg-primary/5 p-3 text-sm">{notice}</div>}

            {showModels && (
              <section className="mt-5 min-w-0 rounded-lg border border-border p-4">
                <h3 className="font-medium">模型列表（{ompModelsByProvider[editor.config.id]?.length ?? 0}）</h3>
                {(ompModelsByProvider[editor.config.id]?.length ?? 0) > 0 ? (
                  <ul className="mt-3 grid min-w-0 grid-cols-1 gap-2 sm:grid-cols-2 xl:grid-cols-3">{ompModelsByProvider[editor.config.id].map((model) => <li key={model.id} className="min-w-0 rounded-md border border-border p-2 text-sm"><span className="block break-all">{model.id}</span><span className="block break-words text-xs text-muted-foreground">{model.name}</span></li>)}</ul>
                ) : <p className="mt-3 text-sm text-muted-foreground">尚未检测到模型。请检查 API 地址、API Key 和 User-Agent。</p>}
              </section>
            )}

            <section className="mt-5 min-w-0">
              <h3 className="mb-2 text-sm font-medium">当前供应商 models.yml 预览（API Key 明文）</h3>
              <pre className="max-h-80 min-w-0 overflow-auto whitespace-pre-wrap break-all rounded-lg border border-border bg-muted/40 p-4 text-xs leading-5">{preview || "填写名称、API 地址和 API Key 后显示 YAML 预览。"}</pre>
            </section>
          </section>
          </>
        )}
        {loading ? (
          <div className="flex items-center gap-2 p-8 text-sm text-muted-foreground"><Loader2 className="h-4 w-4 animate-spin" />读取 OMP 配置…</div>
        ) : providers.length === 0 ? (
          <div className="rounded-xl border border-dashed border-border p-10 text-center text-sm text-muted-foreground">尚未配置 OMP 自定义供应商。</div>
        ) : (
          <div className="grid min-w-0 grid-cols-1 gap-4 lg:grid-cols-2">{providers.map((provider) => <article key={provider.id} className="min-w-0 rounded-xl border border-border bg-card p-4 sm:p-5">
            <div className="flex min-w-0 flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><h2 className="break-all font-semibold">{provider.id}</h2><span className={`rounded-full px-2 py-1 text-xs ${provider.enabled ? "bg-emerald-500/10 text-emerald-600" : "bg-muted text-muted-foreground"}`}>{provider.enabled ? "已启用" : "已禁止"}</span></div><p className="mt-1 break-all text-sm text-muted-foreground">{provider.baseUrl}</p></div><div className="shrink-0 text-right">{provider.enabled && <span className="rounded-full bg-muted px-2 py-1 text-xs">{Object.prototype.hasOwnProperty.call(ompModelsByProvider, provider.id) ? `${ompModelsByProvider[provider.id].length} 个模型` : "未检测"}</span>}</div></div>
            <div className="mt-4 flex flex-wrap gap-2 border-t border-border pt-3"><button onClick={() => beginEdit(provider)} className="inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm hover:bg-accent"><Pencil className="h-3.5 w-3.5" />编辑</button>{provider.enabled ? <button onClick={() => setConfirmDialog({ kind: "disable", provider })} className="rounded-md border border-border px-3 py-1.5 text-sm text-amber-600 hover:bg-amber-500/10">禁止</button> : <button onClick={() => void enableProvider(provider)} className="rounded-md border border-border px-3 py-1.5 text-sm text-emerald-600 hover:bg-emerald-500/10">启用</button>}<button onClick={() => setConfirmDialog({ kind: "delete", provider })} className="inline-flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm text-destructive hover:bg-destructive/10"><Trash2 className="h-3.5 w-3.5" />删除</button>{provider.userAgent && <span className="ml-auto max-w-full truncate self-center text-xs text-muted-foreground">UA: {provider.userAgent}</span>}</div>
          </article>)}</div>
        )}
      </div>
      </main>
      {confirmDialog && (
        <ConfirmModal
          title={confirmDialog.kind === "disable" ? "禁止 OMP 供应商" : "删除 OMP 供应商"}
          description={confirmDialog.kind === "disable"
            ? `“${confirmDialog.provider.id}”将从 OMP models.yml 中移除，但配置会保留在 Viewer 的禁用区，可以再次启用。`
            : `“${confirmDialog.provider.id}”的配置将被永久删除，不再显示在供应商列表中。`}
          confirmLabel={confirmDialog.kind === "disable" ? "禁止" : "删除"}
          destructive={confirmDialog.kind === "delete"}
          busy={confirmBusy}
          onConfirm={() => void confirmAction()}
          onCancel={() => setConfirmDialog(null)}
        />
      )}
    </>
  );
}
