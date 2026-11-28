import { invoke } from "@tauri-apps/api/core";
import { call, readAttachment } from "./ipc/commands";
import type { ImageQuality } from "./ipc/types";

/** Short and long side limits for each setting (720p fits in 1280×720, either orientation). */
const BOXES: Record<ImageQuality, [short: number, long: number]> = {
  "720p": [720, 1280],
  "1080p": [1080, 1920],
  "1440p": [1440, 2560],
};

/** GIF (animation) and SVG (vector) are left alone, like non-image files. */
const RESIZABLE = /^image\/(jpeg|png|webp|heic|heif)$/;

/**
 * Scale an image down to fit the chosen resolution and re-encode it as WebP.
 * Returns the original file when it is not a resizable image, is a WebP that
 * already fits (re-encoding would only lose quality), the WebP is not smaller,
 * or anything in the decode/encode fails.
 */
export async function compressImage(file: File, quality: ImageQuality): Promise<File> {
  if (!RESIZABLE.test(file.type)) return file;
  try {
    const bitmap = await createImageBitmap(file);
    const { width, height } = bitmap;
    const [maxShort, maxLong] = BOXES[quality] ?? BOXES["1080p"];
    const scale = Math.min(
      1,
      maxShort / Math.min(width, height),
      maxLong / Math.max(width, height),
    );
    if (scale === 1 && file.type === "image/webp") {
      bitmap.close();
      return file;
    }
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(width * scale);
    canvas.height = Math.round(height * scale);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no 2d context");
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    bitmap.close();
    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob(resolve, "image/webp", 0.85),
    );
    // Some WebViews silently fall back to PNG when they cannot encode WebP.
    if (!blob || blob.type !== "image/webp" || blob.size >= file.size) return file;
    const stem = file.name.replace(/\.[^.]*$/, "") || "image";
    return new File([blob], `${stem}.webp`, { type: "image/webp" });
  } catch {
    return file;
  }
}

const MIME: Record<string, string> = {
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  webp: "image/webp",
};

let compressing: Promise<boolean> | undefined;

/**
 * Compress the attachment images that did not come through the app (copied
 * into `attachments/` by hand, or older than compression) and replace the
 * original; the backend rewrites the links when the extension changes. Sync
 * holds them back until this has seen them. Resolves to whether a note changed.
 */
export function compressPendingAttachments(quality: ImageQuality): Promise<boolean> {
  compressing ??= (async () => {
    let changed = false;
    try {
      for (const path of await call("attachments_to_compress")) {
        const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
        const name = path.slice(path.lastIndexOf("/") + 1);
        const original = new File([await readAttachment(path)], name, { type: MIME[ext] ?? "" });
        const out = await compressImage(original, quality);
        if (out === original) {
          await call("attachment_mark_checked", { path });
        } else {
          await invoke("attachment_replace", new Uint8Array(await out.arrayBuffer()), {
            headers: {
              "x-path": encodeURIComponent(path),
              "x-ext": out.name.slice(out.name.lastIndexOf(".") + 1),
            },
          });
          changed ||= !out.name.endsWith(`.${ext}`);
        }
      }
    } catch (e) {
      console.warn("attachment compression failed", e);
    } finally {
      compressing = undefined;
    }
    return changed;
  })();
  return compressing;
}
