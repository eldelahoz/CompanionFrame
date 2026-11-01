// Synthesizes src-tauri/assets/notification.wav (original FM crystal ping, 24 kHz mono 16-bit). Regenerate: node scripts/make-notification-sound.mjs
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const SR = 24_000; // must match SAMPLE_RATE in src-tauri/src/services/notify/sound.rs
const DUR = 1.1;
const N = Math.round(SR * DUR);
const out = new Float64Array(N);

// A bell-like FM ping: sine carrier, inharmonic modulator whose index decays faster than the amplitude.
function ping(start, f, amp, decay) {
  const i0 = Math.round(start * SR);
  for (let i = i0; i < N; i++) {
    const t = (i - i0) / SR;
    const att = Math.min(1, t / 0.004);
    const env = att * Math.exp(-t / decay);
    const sweep = f * (1 + 0.012 * Math.exp(-t / 0.05)); // slight settling from above: "tech" ping
    const index = 2.2 * Math.exp(-t / 0.09);
    const body = Math.sin(2 * Math.PI * sweep * t + index * Math.sin(2 * Math.PI * sweep * 2.76 * t));
    const shine = 0.22 * Math.sin(2 * Math.PI * sweep * 5.4 * t) * Math.exp(-t / (decay * 0.3)); // glassy inharmonic top
    const oct = 0.35 * Math.sin(2 * Math.PI * sweep * 2 * t) * Math.exp(-t / (decay * 0.6)); // octave for a pure shimmer
    out[i] += amp * env * (body + shine + oct);
  }
}

// Rising fifth (D6 -> A6), the second note a bit longer.
ping(0, 1174.66, 0.8, 0.22);
ping(0.11, 1760.0, 1.0, 0.3);

// Shimmer: two short delay taps of different lengths widen the tail.
const dry = Float64Array.from(out);
for (const [d, g] of [[0.031, 0.28], [0.047, 0.2]]) {
  const k = Math.round(d * SR);
  for (let i = k; i < N; i++) out[i] += g * dry[i - k];
}

// Fades remove clicks, then normalize the peak to -3 dBFS.
const fi = Math.round(0.002 * SR);
const fo = Math.round(0.05 * SR);
for (let i = 0; i < fi; i++) out[i] *= i / fi;
for (let i = 0; i < fo; i++) out[N - 1 - i] *= i / fo;
const peak = out.reduce((m, v) => Math.max(m, Math.abs(v)), 0);
const k = 10 ** (-3 / 20) / peak;

const buf = Buffer.alloc(44 + N * 2);
buf.write("RIFF", 0);
buf.writeUInt32LE(buf.length - 8, 4);
buf.write("WAVEfmt ", 8);
buf.writeUInt32LE(16, 16);
buf.writeUInt16LE(1, 20);
buf.writeUInt16LE(1, 22);
buf.writeUInt32LE(SR, 24);
buf.writeUInt32LE(SR * 2, 28);
buf.writeUInt16LE(2, 32);
buf.writeUInt16LE(16, 34);
buf.write("data", 36);
buf.writeUInt32LE(N * 2, 40);
let sq = 0;
for (let i = 0; i < N; i++) {
  const s = Math.round(out[i] * k * 32767);
  buf.writeInt16LE(s, 44 + i * 2);
  sq += (s / 32767) ** 2;
}
const file = fileURLToPath(new URL("../src-tauri/assets/notification.wav", import.meta.url));
writeFileSync(file, buf);
console.log(`${buf.length} bytes, ${DUR}s, rms ${(20 * Math.log10(Math.sqrt(sq / N))).toFixed(1)} dBFS`);
