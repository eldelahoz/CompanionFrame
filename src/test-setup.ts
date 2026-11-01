import { configure } from "@testing-library/dom";

// The large view tests render thousands of rows; under a busy machine 1 s waits flake.
configure({ asyncUtilTimeout: 5000 });
