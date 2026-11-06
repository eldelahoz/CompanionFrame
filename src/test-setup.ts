import { configure } from "@testing-library/dom";
import { beforeEach } from "vitest";
import { resetViewStates } from "./stores/viewState";

// The large view tests render thousands of rows; under a busy machine 1 s waits flake.
configure({ asyncUtilTimeout: 5000 });
// View state outlives a view by design, so each test starts from the defaults.
beforeEach(resetViewStates);
// jsdom has no ResizeObserver; the virtual lists only need it to exist (tests give sizes through getBoundingClientRect).
globalThis.ResizeObserver ??= class { observe() {} unobserve() {} disconnect() {} };
