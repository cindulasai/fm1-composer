<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import type { AiInfo, AiProvider } from "./lib/types";

  let { onclose, onchange }: { onclose: () => void; onchange: (active: string | null) => void } = $props();

  let info = $state<AiInfo | null>(null);
  let providerId = $state("anthropic");
  let model = $state("");
  let baseUrl = $state("");
  let key = $state("");
  let status = $state("");
  let bad = $state(false);
  let working = $state(false);

  const provider = $derived<AiProvider | undefined>(info?.providers.find((p) => p.id === providerId));
  const keySaved = $derived(!!info?.has_key && info?.settings?.provider === providerId);

  function show(text: string, isBad = false) {
    status = text;
    bad = isBad;
  }

  function adopt(next: AiInfo) {
    info = next;
    onchange(next.active);
  }

  onMount(async () => {
    try {
      const loaded = await api.aiSettings();
      info = loaded;
      if (loaded.settings) {
        providerId = loaded.settings.provider;
        model = loaded.settings.model;
        baseUrl = loaded.settings.base_url ?? "";
      } else {
        model = loaded.providers.find((p) => p.id === providerId)?.models[0] ?? "";
      }
    } catch (error) {
      show(String(error), true);
    }
  });

  function chooseProvider(id: string) {
    providerId = id;
    const next = info?.providers.find((p) => p.id === id);
    model = next?.models[0] ?? "";
    baseUrl = "";
    key = "";
    show("");
  }

  async function save() {
    working = true;
    try {
      const next = await api.saveAiSettings(
        { provider: providerId, model, base_url: baseUrl.trim() || null },
        key.trim() || null,
      );
      key = "";
      adopt(next);
      show(next.active ? `Saved. "Make it" will use ${next.active}.` : "Saved. Add a key to start using it.");
    } catch (error) {
      show(String(error), true);
    } finally {
      working = false;
    }
  }

  async function test() {
    working = true;
    show("Asking the model…");
    try {
      show(await api.testAi());
    } catch (error) {
      show(String(error), true);
    } finally {
      working = false;
    }
  }

  async function removeKey() {
    working = true;
    try {
      adopt(await api.removeAiKey(providerId));
      show("The key was removed from the system keychain.");
    } catch (error) {
      show(String(error), true);
    } finally {
      working = false;
    }
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) onclose(); }}>
  <div class="panel" role="dialog" aria-modal="true" aria-labelledby="settings-title">
    <header>
      <h2 id="settings-title">AI settings</h2>
      <button class="close" onclick={onclose} aria-label="Close settings">✕</button>
    </header>

    <p class="lead">Choose any language model to read your descriptions. Without one, a plain keyword reader on this computer is used.</p>

    {#if info}
      <label>Provider
        <select value={providerId} onchange={(e) => chooseProvider((e.currentTarget as HTMLSelectElement).value)}>
          {#each info.providers as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
        </select>
      </label>

      <label>Model
        <input list="model-names" bind:value={model} placeholder="model name" spellcheck="false" autocomplete="off" />
        <datalist id="model-names">
          {#each provider?.models ?? [] as name (name)}<option value={name}></option>{/each}
        </datalist>
        <span class="hint">Pick a suggestion or type any model name the provider offers.</span>
      </label>

      {#if provider && (provider.needs_url || provider.default_url || provider.id === "ollama")}
        <label>Address {provider.needs_url ? "" : "(optional)"}
          <input bind:value={baseUrl} placeholder={provider.default_url ?? (provider.id === "ollama" ? "http://localhost:11434" : "https://…/v1")} spellcheck="false" autocomplete="off" />
        </label>
      {/if}

      {#if provider?.needs_key}
        <label>API key
          <input type="password" bind:value={key} placeholder={keySaved ? "Saved in your system keychain (type to replace)" : "Paste your key"} autocomplete="off" spellcheck="false" />
          {#if provider.key_url}<span class="hint">Get a key at {provider.key_url}</span>{/if}
        </label>
      {/if}

      <div class="buttons">
        <button class="primary" onclick={save} disabled={working}>Save</button>
        <button onclick={test} disabled={working || !info.active}>Test</button>
        {#if provider?.needs_key}<button onclick={removeKey} disabled={working} title="Removes this provider's key from the system keychain">Remove key</button>{/if}
      </div>

      <p class="active">Now using: <strong>{info.active ?? "the keyword reader (no AI)"}</strong></p>
      {#if status}<p class="status" class:bad role="status">{status}</p>{/if}
      <p class="privacy">Your key is kept in your system keychain, never in a file. Only the words you type in the description box are sent to the provider.</p>
    {:else if status}
      <p class="status bad" role="alert">{status}</p>
    {:else}
      <p>Loading…</p>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.45); display: flex; align-items: center; justify-content: center; z-index: 10; padding: 16px; }
  .panel { background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; width: min(480px, 100%); max-height: 90vh; overflow-y: auto; padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; }
  header { display: flex; justify-content: space-between; align-items: center; }
  h2 { margin: 0; font-size: 18px; }
  .close { background: none; border: none; color: var(--muted); font-size: 16px; cursor: pointer; }
  .lead, .hint, .privacy { color: var(--muted); font-size: 13px; margin: 0; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--muted); }
  input, select { background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 7px 9px; font-size: 14px; }
  .buttons { display: flex; gap: var(--gap); }
  .buttons button { background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 6px 14px; cursor: pointer; }
  .buttons button.primary { background: var(--accent); color: var(--accent-ink); border-color: var(--accent); }
  .buttons button:disabled { opacity: 0.5; cursor: default; }
  .active { margin: 0; font-size: 13px; }
  .status { margin: 0; font-size: 13px; }
  .status.bad { color: var(--bad); }
</style>
