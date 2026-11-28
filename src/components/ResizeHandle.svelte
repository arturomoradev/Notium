<script lang="ts">
  import { layout, type ResizablePanel } from "../lib/layout.svelte";

  /** Drag handle on the inner edge of a docked side panel (desktop layout only). */
  let { panel }: { panel: ResizablePanel } = $props();

  /** Pointer x and panel width when the drag started; null when not dragging. */
  let start = $state<{ x: number; width: number } | null>(null);

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    start = { x: e.clientX, width: layout.panelWidths[panel] };
  }

  function onPointerMove(e: PointerEvent): void {
    if (!start) return;
    // The sidebar's free edge is on its right, the inspector panel's on its left.
    const dx = e.clientX - start.x;
    layout.setPanelWidth(panel, start.width + (panel === "sidebar" ? dx : -dx));
  }

  function onPointerUp(): void {
    if (!start) return;
    start = null;
    layout.savePanelWidths();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="resize-handle resize-{panel}"
  class:dragging={start !== null}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  ondblclick={() => (layout.setPanelWidth(panel, null), layout.savePanelWidths())}
></div>
<svelte:body class:panel-resizing={start !== null} />
