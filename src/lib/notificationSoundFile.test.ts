import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  NOTIFICATION_SOUND_MAX_INPUT_BYTES,
  NOTIFICATION_SOUND_MAX_SECONDS,
  NOTIFICATION_SOUND_MAX_WAV_BYTES,
  NOTIFICATION_SOUND_SAMPLE_RATE,
} from "../../config/shared/notificationSound";
import { prepareNotificationSound } from "./notificationSoundFile";

const decode = vi.fn<(bytes: ArrayBuffer) => Promise<{ duration: number }>>();
let rendered = new Float32Array(0);

class MockAudioContext {
  decodeAudioData = decode;
  close = vi.fn(async () => {});
}

class MockOfflineAudioContext {
  static created: Array<[number, number, number]> = [];
  destination = {};
  constructor(channels: number, length: number, sampleRate: number) {
    MockOfflineAudioContext.created.push([channels, length, sampleRate]);
  }
  createBufferSource() {
    return { buffer: null as unknown, connect: vi.fn(), start: vi.fn(), disconnect: vi.fn() };
  }
  startRendering = vi.fn(async () => ({ getChannelData: () => rendered }));
}

function file(name: string, size = 1000): File {
  return new File([new Uint8Array(size)], name);
}

// Buffer-like reads over the decoded bytes (no node types in the app tsconfig).
function decoded(data: string) {
  const raw = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
  const view = new DataView(raw.buffer);
  return {
    length: raw.length,
    toString: (_enc: string, start: number, end: number) => String.fromCharCode(...raw.subarray(start, end)),
    readUInt32LE: (o: number) => view.getUint32(o, true),
    readUInt16LE: (o: number) => view.getUint16(o, true),
    readInt16LE: (o: number) => view.getInt16(o, true),
  };
}

beforeEach(() => {
  vi.stubGlobal("AudioContext", MockAudioContext);
  vi.stubGlobal("OfflineAudioContext", MockOfflineAudioContext);
  MockOfflineAudioContext.created = [];
  decode.mockReset().mockResolvedValue({ duration: 0.5 });
  rendered = new Float32Array(0);
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("notification sound picker", () => {
  it.each([
    file("chime.txt"),
    file("chime.wav", 0),
    file("chime.mp3", NOTIFICATION_SOUND_MAX_INPUT_BYTES + 1),
  ])("rejects the wrong extension, an empty file and an oversized file (%#)", async (input) => {
    await expect(prepareNotificationSound(input)).rejects.toThrow("Invalid notification sound");
    expect(decode).not.toHaveBeenCalled();
  });

  it.each([NOTIFICATION_SOUND_MAX_SECONDS + 0.01, 0, NaN])(
    "rejects a decoded duration of %s before rendering",
    async (duration) => {
      decode.mockResolvedValueOnce({ duration });
      await expect(prepareNotificationSound(file("chime.wav"))).rejects.toThrow("duration");
      expect(MockOfflineAudioContext.created).toHaveLength(0);
    },
  );

  it("renders mono PCM at the shared rate with saturating samples", async () => {
    decode.mockResolvedValueOnce({ duration: 5 / NOTIFICATION_SOUND_SAMPLE_RATE });
    rendered = Float32Array.from([-1, 1, 0.5, -1.5, 2]);
    const upload = await prepareNotificationSound(file(`${"n".repeat(130)}.ogg`));
    expect(MockOfflineAudioContext.created).toEqual([[1, 5, NOTIFICATION_SOUND_SAMPLE_RATE]]);
    expect(upload.name).toHaveLength(120);
    const bytes = decoded(upload.data);
    expect(bytes.length).toBe(44 + 10);
    expect(bytes.toString("ascii", 0, 4)).toBe("RIFF");
    expect(bytes.readUInt32LE(4)).toBe(bytes.length - 8);
    expect(bytes.toString("ascii", 8, 16)).toBe("WAVEfmt ");
    expect(bytes.readUInt16LE(22)).toBe(1);
    expect(bytes.readUInt32LE(24)).toBe(NOTIFICATION_SOUND_SAMPLE_RATE);
    expect(bytes.readUInt32LE(28)).toBe(NOTIFICATION_SOUND_SAMPLE_RATE * 2);
    expect(bytes.readUInt32LE(40)).toBe(10);
    expect([0, 1, 2, 3, 4].map((index) => bytes.readInt16LE(44 + index * 2))).toEqual([
      -32768, 32767, 16384, -32768, 32767,
    ]);
  });

  it("accepts exactly the maximum duration and lands on the byte cap", async () => {
    decode.mockResolvedValueOnce({ duration: NOTIFICATION_SOUND_MAX_SECONDS });
    rendered = new Float32Array(NOTIFICATION_SOUND_MAX_SECONDS * NOTIFICATION_SOUND_SAMPLE_RATE);
    const upload = await prepareNotificationSound(file("chime.wav"));
    expect(decoded(upload.data).length).toBe(NOTIFICATION_SOUND_MAX_WAV_BYTES);
  });
});
