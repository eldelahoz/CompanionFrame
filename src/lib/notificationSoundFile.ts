import type { SoundUpload } from "./ipc";
import {
  NOTIFICATION_SOUND_MAX_INPUT_BYTES,
  NOTIFICATION_SOUND_MAX_SECONDS,
  NOTIFICATION_SOUND_SAMPLE_RATE,
} from "../../config/shared/notificationSound";
import { pcm16WavBytes } from "../../config/shared/wav";

export async function prepareNotificationSound(file: File): Promise<SoundUpload> {
  if (
    !/\.(wav|mp3|ogg)$/i.test(file.name) ||
    file.size === 0 ||
    file.size > NOTIFICATION_SOUND_MAX_INPUT_BYTES
  ) {
    throw new Error("Invalid notification sound file");
  }

  const context = new AudioContext();
  let decoded: AudioBuffer;
  try {
    decoded = await context.decodeAudioData(await file.arrayBuffer());
  } finally {
    await context.close();
  }
  if (
    !Number.isFinite(decoded.duration) ||
    decoded.duration <= 0 ||
    decoded.duration > NOTIFICATION_SOUND_MAX_SECONDS
  ) {
    throw new Error("Invalid notification sound duration");
  }

  // Store a bounded PCM copy without depending on the source file or its codec.
  const frameCount = Math.ceil(decoded.duration * NOTIFICATION_SOUND_SAMPLE_RATE);
  const renderer = new OfflineAudioContext(1, frameCount, NOTIFICATION_SOUND_SAMPLE_RATE);
  const source = renderer.createBufferSource();
  source.buffer = decoded;
  source.connect(renderer.destination);
  source.start();
  let rendered: AudioBuffer;
  try {
    rendered = await renderer.startRendering();
  } finally {
    source.disconnect();
  }

  const samples = rendered.getChannelData(0);
  const pcm = new Int16Array(samples.length);
  for (let index = 0; index < samples.length; index++) {
    const sample = Math.max(-1, Math.min(1, samples[index] ?? 0));
    pcm[index] = Math.round(sample * (sample < 0 ? 32768 : 32767));
  }
  const bytes = pcm16WavBytes(pcm, NOTIFICATION_SOUND_SAMPLE_RATE);

  const chunks: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += 0x8000) {
    chunks.push(String.fromCharCode(...bytes.subarray(offset, offset + 0x8000)));
  }
  return { name: file.name.slice(0, 120), data: btoa(chunks.join("")) };
}
