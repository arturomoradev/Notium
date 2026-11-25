<script lang="ts">
  import { icons } from "../../lib/icons";

  interface Props {
    name: string;
    size?: number;
  }

  let { name, size = 20 }: Props = $props();
  const markup = $derived(icons[name] ?? "");
</script>

<svg
  class="notium-icon"
  style="--icon-size: {size}px"
  viewBox="0 0 24 24"
  width={size}
  height={size}
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <!-- `markup` comes from the static icon table in lib/icons.ts, never from user content. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html markup}
</svg>
