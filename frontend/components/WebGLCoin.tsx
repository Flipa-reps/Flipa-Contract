import React, { useEffect, useRef } from "react";
import type { CoinFace, CoinFlipState } from "./CoinFlip";

interface WebGLCoinProps {
  result?: CoinFace;
  state?: CoinFlipState;
  onAnimationEnd?: () => void;
  size?: number;
}

// Vertex shader: positions a quad and passes UV coords
const VERT_SRC = `
  attribute vec2 a_pos;
  varying vec2 v_uv;
  void main() {
    v_uv = a_pos * 0.5 + 0.5;
    gl_Position = vec4(a_pos, 0.0, 1.0);
  }
`;

// Fragment shader: draws a 3D-looking coin with rotation
const FRAG_SRC = `
  precision mediump float;
  varying vec2 v_uv;
  uniform float u_angle;   // current Y-rotation in radians
  uniform float u_time;
  uniform int u_face;      // 0 = heads, 1 = tails

  vec3 hsv2rgb(vec3 c) {
    vec4 K = vec4(1.0, 2.0/3.0, 1.0/3.0, 3.0);
    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);
  }

  void main() {
    vec2 uv = v_uv * 2.0 - 1.0;
    float dist = length(uv);

    // Coin boundary
    if (dist > 1.0) { discard; }

    // Perspective squish based on Y-rotation
    float cosA = cos(u_angle);
    float squish = abs(cosA);
    if (abs(uv.x) > squish + 0.01) { discard; }

    // Determine which face is showing
    bool showFront = cosA >= 0.0;
    bool isHeads = (u_face == 0);
    bool showHeads = (showFront == isHeads) || (showFront != isHeads && cosA < 0.0);
    // Simpler: front face when cosA >= 0
    bool headsVisible = (cosA >= 0.0) ? (u_face == 0) : (u_face == 1);

    // Base coin color
    vec3 gold = vec3(0.85, 0.65, 0.13);
    vec3 silver = vec3(0.75, 0.75, 0.80);
    vec3 baseColor = headsVisible ? gold : silver;

    // Rim lighting
    float rim = 1.0 - smoothstep(0.85, 1.0, dist);
    float edge = smoothstep(0.92, 1.0, dist);
    vec3 rimColor = mix(baseColor, vec3(1.0), edge * 0.4);

    // Specular highlight
    vec2 lightDir = normalize(vec2(-0.5, 0.8));
    float spec = pow(max(dot(normalize(uv), lightDir), 0.0), 8.0) * 0.6;

    // Face symbol
    float symbol = 0.0;
    if (headsVisible) {
      // "H" glyph approximation
      float bar = step(abs(uv.y), 0.08) * step(abs(uv.x), 0.35);
      float leftLeg = step(abs(uv.x + 0.28), 0.06) * step(abs(uv.y), 0.45);
      float rightLeg = step(abs(uv.x - 0.28), 0.06) * step(abs(uv.y), 0.45);
      symbol = clamp(bar + leftLeg + rightLeg, 0.0, 1.0);
    } else {
      // "T" glyph approximation
      float top = step(abs(uv.y - 0.35), 0.07) * step(abs(uv.x), 0.38);
      float stem = step(abs(uv.x), 0.06) * step(abs(uv.y + 0.05), 0.45);
      symbol = clamp(top + stem, 0.0, 1.0);
    }

    vec3 color = mix(rimColor, vec3(0.2, 0.15, 0.05), symbol * 0.7);
    color += spec;

    // Depth shading
    float depth = 1.0 - dist * 0.3;
    gl_FragColor = vec4(color * depth, 1.0);
  }
`;

function compileShader(gl: WebGLRenderingContext, type: number, src: string): WebGLShader {
  const shader = gl.createShader(type)!;
  gl.shaderSource(shader, src);
  gl.compileShader(shader);
  return shader;
}

function createProgram(gl: WebGLRenderingContext): WebGLProgram {
  const prog = gl.createProgram()!;
  gl.attachShader(prog, compileShader(gl, gl.VERTEX_SHADER, VERT_SRC));
  gl.attachShader(prog, compileShader(gl, gl.FRAGMENT_SHADER, FRAG_SRC));
  gl.linkProgram(prog);
  return prog;
}

export function WebGLCoin({
  result = "heads",
  state = "idle",
  onAnimationEnd,
  size = 200,
}: WebGLCoinProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number>(0);
  const startRef = useRef<number | null>(null);
  const doneRef = useRef(false);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const gl = canvas.getContext("webgl");
    if (!gl) return;

    const prog = createProgram(gl);
    gl.useProgram(prog);

    // Full-screen quad
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]),
      gl.STATIC_DRAW
    );
    const aPos = gl.getAttribLocation(prog, "a_pos");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

    const uAngle = gl.getUniformLocation(prog, "u_angle");
    const uTime = gl.getUniformLocation(prog, "u_time");
    const uFace = gl.getUniformLocation(prog, "u_face");

    const FLIP_DURATION = 1200; // ms
    const faceValue = result === "heads" ? 0 : 1;

    doneRef.current = false;
    startRef.current = null;

    function render(ts: number) {
      if (!startRef.current) startRef.current = ts;
      const elapsed = ts - startRef.current;

      let angle = 0;

      if (state === "flipping") {
        // Ease-in-out spin: multiple full rotations
        const t = Math.min(elapsed / FLIP_DURATION, 1);
        const eased = t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t;
        angle = eased * Math.PI * 6; // 3 full rotations

        if (t >= 1 && !doneRef.current) {
          doneRef.current = true;
          onAnimationEnd?.();
        }
      } else if (state === "revealed") {
        // Settle to final face
        angle = faceValue === 0 ? 0 : Math.PI;
      }

      gl!.viewport(0, 0, canvas!.width, canvas!.height);
      gl!.clearColor(0, 0, 0, 0);
      gl!.clear(gl!.COLOR_BUFFER_BIT);
      gl!.uniform1f(uAngle, angle);
      gl!.uniform1f(uTime, elapsed / 1000);
      gl!.uniform1i(uFace, faceValue);
      gl!.drawArrays(gl!.TRIANGLE_STRIP, 0, 4);

      if (state === "flipping" && !doneRef.current) {
        rafRef.current = requestAnimationFrame(render);
      }
    }

    rafRef.current = requestAnimationFrame(render);
    return () => cancelAnimationFrame(rafRef.current);
  }, [state, result, onAnimationEnd]);

  return (
    <canvas
      ref={canvasRef}
      width={size}
      height={size}
      style={{ display: "block" }}
      aria-label={
        state === "revealed"
          ? `Coin landed on ${result}`
          : state === "flipping"
          ? "Coin is flipping"
          : "Coin"
      }
      aria-live="polite"
    />
  );
}
