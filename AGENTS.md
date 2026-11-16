# Working on basketman

A basketball manager game in Rust: you run a club (squad, tactics, staff, finances) and coach matches that are simulated on a real floor and shown in 3D, 2D or as commentary. It runs as a desktop app, in the browser (wasm) and from the command line, where AI agents can also play it.

## Layout

- `crates/core` — the whole game as a deterministic library: world, season, match engine (`engine/`), finances, health, tactics. No I/O.
- `crates/cli` — the `basketman` binary: simulate seasons, lab statistics, and the agent interfaces (`basketman play`, `basketman mcp`; see `docs/agents.md`).
- `crates/web` — the `basketman-web` app (eframe/egui): desktop window natively, wasm in the browser. The 3D view is drawn on the CPU in `src/view3d/`.
- `docs/` — design notes. `docs/engine-physics.md` is the plan the match engine is moving toward.
- `tools/` — asset scripts. `viewer3d/` — an older experimental viewer, not needed.

## Build and run

Needs a current stable Rust (edition 2024, so 1.85 or newer). No nightly features are used.

```sh
cargo run --release -p basketman-web        # the game in a desktop window
cargo run --release -p basketman-cli -- 5   # simulate from seed 5 and print the result
cargo test --release -p basketman-core
cargo test --release -p basketman-web
```

Always build with `--release`; the simulation is slow in debug.

macOS and Windows: nothing in the code is platform-specific and eframe supports both, but the game has so far only been built and run on Linux and in Chromium. If something fails on a Mac, that is a real finding, not your setup.

Linux needs the usual windowing libraries for eframe (X11 or Wayland, libxkbcommon, OpenGL).

Browser build:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
cd crates/web && trunk serve --release      # http://127.0.0.1:8080
```

## Checking your work without a person at the window

`basketman-web` can draw one screen to a PNG and exit:

```sh
./target/release/basketman-web --screenshot out.png match live
SHOT_WIDTH=390 SHOT_HEIGHT=844 ./target/release/basketman-web --screenshot phone.png squad
```

The arguments are a screen name and its options (`title`, `squad`, `tactics`, `match`, `match live`, `finances`, `club`, ...). Useful variables: `SHOT_WIDTH`, `SHOT_HEIGHT`, `SHOT_SCROLL=0`, `SHOT_CLICK="x,y;x,y"` (clicks before the picture is taken), `SHOT_VIEW=3d|flat|commentary`, `SHOT_SEASONS=n`. Look at the picture before saying a UI change works.

## Rules that are easy to break

1. **Determinism.** The same seed and the same commands must give the same game on every machine. In `crates/core`: use the game's own random numbers, never the system's; no `powf`, `exp`, `ln` or trigonometry from the standard library in simulation code (they differ between platforms); no iteration over hash maps where order matters. The check is that the serial and the parallel build print the same thing:

   ```sh
   cargo run --release -p basketman-cli -- 5 | md5sum
   cargo run --release -p basketman-cli --no-default-features --target-dir target/serial -- 5 | md5sum
   ```

2. **One way to change the world.** The app and the agents change the game only through `World::apply(Command)` in `crates/core/src/command.rs`. A new action is a new `Command`, not a direct edit of the world from the UI.

3. **The picture shows only what the simulation knows.** The 3D view may smooth between frames but must not invent motion or outcomes. If something looks wrong because the engine does not model it, the fix belongs in the engine.

4. **Physics over dice.** The engine is moving from weighted dice rolls to bodies, hands and a physical ball (`docs/engine-physics.md`). Prefer geometry to a new probability, and then measure: `basketman lab` prints league statistics, and a change that looks good but shifts them out of a realistic range is not done.

5. **Tests state what should be true.** When a test fails after an engine change, find out why. Do not loosen the assertion or change the seed until it passes.

6. **Saves must keep loading.** New fields on saved types get `#[serde(default)]`; there is an old-save test in `crates/core/src/save.rs`.

7. **Tactics change only when play is stopped.** In a live game (`LiveRound::input` in `crates/core/src/league.rs`) orders, instructions and talk are refused unless the game is in a timeout (either team's), a break, or before the tip; in live play only a timeout, queued substitutions and the assistant's substitution switch are allowed. The AI benches follow the same rule. Never add a way around it in the UI; the coaching room (`crates/web/src/matchview/stoproom.rs`) is where changes are made.
