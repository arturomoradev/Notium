<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { mountEditor, type NoteEngine, type EditorViewMode } from "./noteEngine";
  import type { EditorHandle } from "./engine";

  interface Props {
    content: string;
    viewMode: "edit" | "reading";
    sourceMode: boolean;
    onChange: () => void;
    onLinkActivate: (target: string) => void;
    title?: string;
    onRenameTitle?: (title: string) => Promise<boolean>;
    onReady: (handle: EditorHandle) => void;
    onEditorDestroy: (handle: EditorHandle) => void;
  }

  let {
    content,
    viewMode,
    sourceMode,
    onChange,
    onLinkActivate,
    title,
    onRenameTitle,
    onReady,
    onEditorDestroy,
  }: Props = $props();

  let host: HTMLDivElement;
  let engine: NoteEngine | undefined;
  let registeredHandle: EditorHandle | undefined;

  function syncMode(vm: "edit" | "reading", sm: boolean): void {
    if (!engine) return;
    const m: EditorViewMode = vm === "reading" ? "reading" : sm ? "source" : "live";
    engine.setMode(m);
    if (vm === "reading") engine.blur();
  }

  onMount(() => {
    engine = mountEditor(host, { onChange, onLinkActivate, title, onRenameTitle }, "live");
    registeredHandle = {
      getContent: () => engine?.getContent() ?? "",
      getPlainText: () => engine?.getPlainText() ?? "",
      focus: () => engine?.focus(),
      blur: () => engine?.blur(),
      getEngine: () => engine,
      setViewMode: (mode: "edit" | "reading", source = false) => syncMode(mode, source),
      scrollToHeading: (heading: string) => engine?.scrollToHeading(heading) ?? false,
    };
    onReady(registeredHandle);
    engine.setContent(content);
    syncMode(viewMode, sourceMode);
  });

  $effect(() => {
    syncMode(viewMode, sourceMode);
  });

  onDestroy(() => {
    if (registeredHandle) onEditorDestroy(registeredHandle);
    registeredHandle = undefined;
    engine?.destroy();
    engine = undefined;
  });
</script>

<div class="editor-engine" bind:this={host}></div>
