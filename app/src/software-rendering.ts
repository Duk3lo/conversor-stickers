// Software GL (SwiftShader, llvmpipe) cannot take hardware-decoded video frames: in Waydroid the
// whole window turns black once a <video> is shown. Reading pixels back does not detect it (the
// canvas is tainted and black), so the renderer name is the only reliable signal.
const SOFTWARE = ["swiftshader", "llvmpipe", "softpipe", "basic render driver", "software rasterizer"];

export function isSoftwareRenderer(renderer: string | null): boolean {
  if (renderer === null) return false;
  const name = renderer.toLowerCase();
  return SOFTWARE.some((marker) => name.includes(marker));
}

function detect(): boolean {
  let gl: WebGLRenderingContext | null = null;
  try {
    gl = document.createElement("canvas").getContext("webgl");
    const info = gl?.getExtension("WEBGL_debug_renderer_info");
    if (!gl || !info) return false;
    const renderer: unknown = gl.getParameter(info.UNMASKED_RENDERER_WEBGL);
    return isSoftwareRenderer(typeof renderer === "string" ? renderer : null);
  } catch {
    return false;
  } finally {
    // Browsers cap live WebGL contexts, so the probe must not keep one.
    try {
      gl?.getExtension("WEBGL_lose_context")?.loseContext();
    } catch {
      // Releasing is best effort; the answer is already known.
    }
  }
}

let software: boolean | undefined;

/** Computed once. False when WebGL or the debug extension is missing: without evidence, the video is shown. */
export function usesSoftwareRendering(): boolean {
  software ??= detect();
  return software;
}
