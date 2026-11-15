# basketman 3D match viewer (prototype)

A 3D playback of one recorded game, written in the Plush language
(checkout at `/steam/rotko/plush`). It reads a
replay file exported by the `basketman` CLI and draws the court, ten jointed
player figures built to each player's measurements, and the ball, with a
small software renderer.

## Export a game

```sh
export RUSTUP_TOOLCHAIN=nightly-2026-10-06
cargo build --release -p basketman-cli --target-dir target/viewer
./target/viewer/release/basketman --export-game 5 /tmp/game5.bmr
```

The game is the first fixture of `World::generate(seed, 12)`, simulated with
`Rng::new(seed)`. The same seed always gives the same file. Without
`--export-game` the CLI behaves as before.

## Run the viewer

```sh
cd viewer3d
/steam/rotko/plush/target/release/plush match.psh /tmp/game5.bmr
```

Options: `--width`, `--height` (default 960x540), `--cam 1..4`, `--at SECONDS`,
`--fps` (print the frame rate), `--show-help`.

A file in the old `BMREPLAY 1` format is refused with a message asking for a
new export (the old table had only height and shoulder width).

Debug options, mostly for screenshots:

| Option | Effect |
| --- | --- |
| `--focus N` | close-up camera on floor slot N (0-4 home, 5-9 away). `--yaw` is then relative to his facing (0 = from the front, 1.57 = from his left, 3.14 = from behind), `--pitch` the elevation, `--fdist` the distance in metres (default 5) |
| `--lineup 1/2/3` | ignore the game and stand the ten players on the floor in a row, facing the camera / to the right (profile) / away. Same ten as on the floor at `--at`, so builds can be compared |
| `--pose P` | with `--lineup`: 0 stand, 1 defensive stance, 2 run, 3 walk, 4 stance shuffle, 5 run dribbling (slot 4), 6 stand holding the ball (slot 4) |
| `--phase RAD` | with `--lineup`: gait phase |
| `--seq N --seqstep S` | with `--screenshot name.tga`: write N pictures `name_00.tga ...` S seconds apart starting at `--at` (use `magick montage` for a contact sheet) |
| `--nohud` | leave the HUD out of screenshots |

Screenshot without a window:

```sh
plush match.psh game.bmr --outdir /some/existing/dir --screenshot a.tga --at 600 --cam 1
magick a.tga a.png
```

The file is a TGA. `--outdir` is needed when the target is outside the
current directory, because Plush only writes below the current directory or
to directories named on the command line.

Renderer timing without a window: `plush match.psh game.bmr --bench 100 --cam 1`.

## Controls

| Key | Action |
| --- | --- |
| SPACE or P | pause / resume |
| 1, 2, 4 | speed 1x, 2x, 4x |
| LEFT, RIGHT | seek -5 s, +5 s |
| DOWN, UP | seek -30 s, +30 s |
| R | restart |
| C | next camera |
| B, O, T, E | broadcast, orbit, top-down, end-line camera |
| A, D, W, S | orbit camera: turn and tilt |
| Z, X | orbit camera: zoom in, out |
| H | show or hide the help overlay |
| Q or ESC | quit |

The bindings are also printed to the terminal on start. The help overlay is
shown for the first six seconds.

## Files

- `match.psh`: the viewer (loader, motion tables, camera, rasteriser, court, figures, HUD).
- `monogram_font.psh`: the Monogram bitmap font, copied unchanged from the
  Plush examples (CC0).

## File format (`BMREPLAY 2`)

A text file, one record per line, UTF-8. About 800 KB for a 12000 frame game.
Plush parses it in about 40 ms, so no binary format was needed. Version 2
differs from version 1 only in the player table (and the first line).

```
BMREPLAY 2
rate 5
teams <home name>|<away name>
players N
<player line>                                       (N lines, home first)
frames F
<frame and modifier lines>
events E
<time cs>|<text>                                    (E lines, Event::text)
```

Player line, fields separated by `|`:

```
side|name|id|height|shoulder|weight|wingspan|reach|hand|hand side|speed|vertical|strength|stamina|handling
```

- `side` 0 home, 1 away; `id` the `PlayerId` (the viewer seeds skin, hair and
  shoe looks from it).
- `height`, `shoulder` (shoulder width), `wingspan`, `reach` (standing reach),
  `hand` (hand length): centimetres, from `Body`.
- `weight`: kilograms. `hand side`: 0 right-handed, 1 left-handed (`Body::hand`).
- `speed vertical strength stamina handling`: the 1..20 rating times ten
  (so 146 is 14.6). The viewer uses speed, vertical, strength and (slightly)
  stamina; handling is exported for later use.

Frame lines hold integers separated by spaces: `a holder v0 .. v22`.

- `a` is the frame step in centiseconds minus 20.
- `holder` is the floor slot (0..9) of the ball carrier, or -1.
- `v0 v1 v2` are the ball x, y and height in cm; `v3..v22` are x, y of the ten
  players. Court coordinates: x along the court 0..2800, y across 0..1500,
  home attacks x = 28 m.
- The values are second differences: add each to the running velocity of
  that slot, then add the velocity to the position. The first frame starts
  from zero.

Lines before a frame line change state for it and the frames after it:

- `w i0 .. i9`: who is on the floor, as indices into the player table
  (home five, then away five). Substitutions appear as a new `w` line.
- `m period score0 score1 offense`: period (0 based), score, side with the ball.
  `offense` is the side that has the ball (it matched the holder's side in
  every frame that has a holder in the test game), so nothing extra had to be
  exported for the defenders' stance.
- `c clock_cs`: game clock, written only when it differs from the previous
  clock minus the frame step.

Frame time is the running sum of `a + 20`. The shot clock and the ball
holder's identity beyond the slot are not exported.

## Figures

Each player is 19 tapered boxes (torso/jersey, shorts block, neck, head, hair
cap, upper arm, forearm, hand, upper thigh in shorts, lower thigh, shin, shoe;
arms, thighs, shins and shoes twice). Jersey and shorts take the team colour.
Skin (six tones plus a small brightness shift), hair (five tones, one in nine
shaved) and shoes (white, black or yellow) come from the player id, so a
player looks the same in every game.

Dimensions, all from the player's own numbers (`build_dims`, H = height):

- Wingspan and shoulder width fix the arm: shoulder joints sit 0.46 x shoulder
  width off the middle, the arm (shoulder joint to finger tip) is half the
  wingspan minus that; hand length is `hand`, the rest splits 56 % upper arm,
  44 % forearm. A defender's arms out sideways therefore span his wingspan.
- Standing reach sets the shoulder height: 0.815 H + (reach / H - 1.33) H,
  limited to 0.78..0.85 H. Hip height is 0.525 H minus half of the shoulder
  height's difference from 0.82 H (long torso, slightly shorter legs). Torso
  length is shoulder minus hip height; thigh and shin split the rest of the
  leg 50.5 / 49.5 after a 3.6 % H ankle. Head 0.13 H.
- Torso width is the shoulder width (chest 0.45 x, waist 0.35 x on each side).
- Build `tm` = (0.9 + 0.075 (BMI - 22)) x (0.93 + 0.012 x strength rating),
  limited to 0.8..1.4, with BMI = weight / H squared. It scales torso depth and
  width, thigh and shin thickness; arms use its square root.
- Stride length derives from leg length and speed (below); speed rating
  stretches it by 1.2 % per rating point from 10, vertical rating scales knee
  lift and body bounce, stamina adds a slight forward lean when low.

Shading uses one fixed light direction and the face normals of each box
(tapers included); faces turned away from the camera are skipped.

Depth order: every segment is an item in the same list as the ball, hoops and
set pieces, sorted far to near by the depth of its middle along the viewing
direction (small per-part biases put the jersey over the shorts and hair over
the head). That is correct for the usual overlaps (limbs against torso,
neighbouring players in the lane); long boxes that wrap each other can still
sort wrongly for a frame.

## Motion

Everything is derived from the recorded positions (`build_motion`, once at
load, about 170 ms for a full game), so any time can be rendered directly:

- Velocity: central difference over neighbouring frames, smoothed 1-2-1 and
  not carried across substitutions, jumps over 3.5 m or time-out gaps.
  Interpolated between frames for drawing.
- Gait phase: advanced by distance travelled divided by the stride length
  `S = sigma x leg length / duty`, where `duty` is the share of a cycle a foot
  spends on the floor (0.62 walking down to 0.33 running) and `sigma` the
  distance a planted foot travels relative to the hips in leg lengths
  (0.78..0.93, more when sprinting). Because a planted foot must travel
  `v x duty x cycle time`, this keeps planted feet fixed in the world (checked
  numerically at 6.9 m/s: planted-foot speed 0.00-0.01 m/s) and gives a tall
  player fewer, longer strides.
- Legs: foot targets follow a stance/swing path along the direction of travel;
  hips drop just enough that the planted foot at the end of its sweep is within
  reach, with a small vault over the planted foot; knees by two-bone IK with the
  knee pointing forward. Feet pitch heel-first / toe-off and are lifted so the
  sole stays on the floor.
- Arms swing opposite to the legs, elbows flex more with speed, shoulders twist
  against the hips; trunk leans forward with speed. A blend `g` (stand to
  moving, by speed 0.12..0.65 m/s) and `ra` (walk to run, 1.6..4.4 m/s) fades
  stride, swing and lean in.
- Facing: the direction of travel when moving, the ball when standing, smoothed
  per frame, so figures do not spin when almost still.
- Defensive stance: a defender (side without the ball) in the half court the
  other side is attacking, within 3.4 m of an attacker and slower than 4.5 m/s,
  blends in (about 0.2 s) to: hips and knees low, wide feet, trunk leaning
  forward, arms out to the sides, facing the man with the ball if he is within
  6 m, otherwise his nearest attacker. Moving in it gives sideways shuffle
  steps (feet open and close along the direction of travel) instead of a run.
- Ball handler: when the recording has the ball held (holder set and ball
  height about 1 m) the ball is at the hand on his shooting side. Standing, it
  is held in both hands at chest height; moving, it is dribbled: one bounce per
  stride cycle running (two when walking), the hand follows it down and up, the
  arm reaches by IK (and the ball is pulled into reach if the hand cannot get
  to it).

Jumping, shooting, passing, rebounding, layups, tiredness: not animated. The
recording does not say when they happen, and they are not faked: the ball
leaves the hand and flies on its recorded track, the player keeps running.

### What the game would have to export to drive those

Per player per frame an action tag and its phase, for example
`action` (none, jump, shot, pass, catch, layup, rebound, block, screen, fall)
and `phase` 0..255 (progress through the action), plus for the shot or pass the
target (basket end or receiving slot) and the release frame. With that the viewer
can play a take-off, arm raise and release instead of a run cycle, and attach
the ball to the shooter's hands until release. Player vertical position (jump
height) is needed too; today only the ball has a height. A per-frame fatigue
value would drive posture and stride for tiredness.

## Rendering notes

Flat shaded convex polygons, back-face culled, near-plane clipped, drawn with
the painter's algorithm. Court, lines and shadows first, then figure segments,
set pieces and the ball sorted far to near by depth along the view direction. Each polygon is
filled with one native `fill_u32` call per scanline, which is what makes this
fast in an interpreter. Positions are interpolated linearly between frames
and snapped when a player is substituted or jumps more than 3.5 m in one frame.

## Performance

On the development machine, from `--bench 200` (renderer only, broadcast
camera): boxes before / figures now: 2.1 / 3.4 ms at 960x540, 3.5 / 5.1 ms at
1920x1080. Interactive at 960x540 stays at 60 fps (vsync limited, 3.3 ms of
rendering per frame). Loading: 36 ms parse plus about 170 ms for the motion
tables.

## Not done

- Jumping, shooting, passing, rebounding, layups and tiredness (see above).
  Jersey numbers and faces are missing.
- The held ball jumps to the recorded track at the moment of release.
- No stands, crowd or sidelines furniture; the net is eight lines.
- Painter's sorting uses segment middles, so long boxes very close to each
  other can occasionally overlap wrongly. No z-buffer.
- Knees bend more than a real runner's at the end of a long stride, and the
  hands are small boxes; the style is low-poly on purpose.
- No transparency: shadows are opaque patches and cover the floor lines.
- No sound, no mouse control, no window resize.
- Only the first fixture of a generated world can be exported; there is no
  way yet to export a game from a save.
- The score updates when play resumes after a basket, as recorded, not at
  the moment the ball goes through.
- Plush programs run as native programs on the SDL2 VM. This viewer does not
  run in a web browser.
