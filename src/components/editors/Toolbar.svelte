<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { ToolbarAction } from "./toolbar";

  interface Props {
    actions: ToolbarAction[];
  }

  let { actions }: Props = $props();

  // `preventDefault()` on `mousedown` (NOT `touchstart`) so tapping a toolbar
  // button never steals focus from the editor or closes Gboard, while still
  // letting normal touch gestures through on Android. The editor engines
  // re-focus themselves inside each action.
  function run(a: ToolbarAction, e: Event): void {
    e.preventDefault();
    a.run();
  }

  // The strip only scrolls sideways and hides its scrollbar: map the mouse wheel to it.
  // Non-passive listener, so preventDefault can stop the page from scrolling too.
  function wheelToHorizontal(el: HTMLElement) {
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX) || el.scrollWidth <= el.clientWidth) return;
      e.preventDefault();
      el.scrollLeft += e.deltaY;
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return { destroy: () => el.removeEventListener("wheel", onWheel) };
  }
</script>

<div class="format-pill" role="toolbar" aria-label="Formato">
  <div class="pill-scroll" use:wheelToHorizontal>
    {#each actions as a (a.id)}
      <button
        type="button"
        class="pill-btn"
        class:active={a.active ?? false}
        title={a.title}
        aria-label={a.title}
        onpointerdown={(e) => e.preventDefault()}
        onclick={(e) => run(a, e)}
      >
        <Icon name={a.icon ?? "file"} />
      </button>
    {/each}
  </div>
</div>
