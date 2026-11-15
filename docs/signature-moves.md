# Signature moves

Written 2026-11-15 as the design input for the "moves" engine task. The catalogue is compiled from memory, not from sources, and is a first pass, not a complete list. The real players named are references for the designer only. Nothing in the game uses a real name or a real nickname: moves get plain descriptive names.

## 1. What a signature move is

A signature move is something a player was born able to do and that cannot be taught. Training never grants one. It sits beside the ordinary moves, which every player can attempt and which depend on his attributes.

Rules:

1. **Innate.** Assigned when the player is generated. Never learned, never trained, never lost.
2. **Rare.** About one player in eight has one. About one in sixty has two. Three is a once-a-generation player. The chance rises with potential.
3. **Plausible.** A move is only given to a body and skill set that could do it: a sweeping hook needs reach, a shot from the logo needs a real three-point shot. The move stays for life, but the body it needs may not: a poster dunker with no legs left at 36 still knows how and can no longer do it.
4. **Bounded.** A signature makes one situation clearly better. It does not make a poor player good. As a guide, it is worth the difference between an average and a very good shot in the situation it applies to, a few times a game.
5. **Counterable.** Every move has an answer a coach can choose: a scheme, a matchup, a kind of defender. This is what makes it a tactical fact and not only a bonus.
6. **Hidden until seen.** A signature shows on a player's profile once your scouts have watched him enough, or once he does it in a game you have film of.
7. **Simulation first.** The 3D view may only show a move the simulation actually performed. Each move needs its state in the engine before it gets an animation.
8. **It sells.** A player with a signature is more popular than his numbers alone would make him. That feeds attendance, shirt sales and sponsor value once those exist.

## 2. Data

- `enum Signature` in `player.rs`, one variant per move below, with a plain name and a one-line description.
- `Player::signatures: Vec<Signature>`, at most three, `#[serde(default)]`.
- Each variant declares the situation it applies to, its prerequisites, and its counter, so the profile page, the scout report, the commentary and the assistant's advice can all read from one place.

## 3. Catalogue

Columns: the plain name used in the game; who it is modelled on; what it does in the simulation; what answers it.

### Shots

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| Sweeping hook | Kareem Abdul-Jabbar | A hook released above the defender's reach: the contest counts only from a defender taller and longer than the shooter. Works out to about 4 m. | Deny the catch, front the post, send a second defender before the turn. |
| One-legged fadeaway | Dirk Nowitzki | A fadeaway off one foot that creates its own space: the defender's contest is cut sharply, the shooter cannot be blocked from in front. | A smaller, quicker defender crowding the catch so he never gets to his spot. |
| Off-balance leaner | Nikola Jokic | A one-legged shot off the wrong foot going away from the basket. Slow players get a clean look without beating their man. | Stay down, do not jump at the fake, send help from the baseline side. |
| Turnaround fadeaway | Michael Jordan, Kobe Bryant | From the post or the elbow, a turn either way into a fade. The defender cannot sit on one shoulder. | Length. A defender with reach to bother it without leaving the floor. |
| Glass specialist | Tim Duncan | Bank shots from the wings and the block go in more often than the same shot straight on. | Force him to the middle of the floor. |
| Step-back three | James Harden, Luka Doncic | A step back behind the line that opens space off the dribble. The three is taken as if lightly contested. | Pick him up higher, crowd the gather hand, switch a long defender onto him. |
| Logo range | Stephen Curry, Damian Lillard | A real shot from two metres behind the line. The defense has to guard him from half court, which opens the floor for everyone else. | Pick up at half court and trap the ball screen, accepting the four-on-three behind it. |
| Quick release | Klay Thompson | Catch-and-shoot threes leave before a closing defender arrives: the close-out counts for less. | Top-lock him and chase over every screen so he never catches it facing the basket. |
| Shooter on the move | Reggie Miller, Ray Allen, Richard Hamilton | No penalty for shooting straight off a sprint around a screen. Tires his defender. | A fit, disciplined chaser. Switch the screens. |
| Teardrop | Tony Parker | A floater released early over a waiting big man. The rim protector's contest counts for little. | Bring the big man up to the level of the screen so there is no room to float it. |
| Finger roll | George Gervin | A high, soft finish at full stretch that goes over shot blockers on the drive. | Meet him early, body him before the gather. |
| Mid-range pull-up | Chris Paul | Pull-up jumpers from the elbows are as good as his open ones. Punishes a big man who drops back. | Do not drop. Switch or hedge. |
| Underhand free throws | Rick Barry | A fixed, very high free-throw percentage regardless of his shooting. A little embarrassment: young players may refuse to do it. | None. Do not foul him. |

### Off the dribble

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| Killer crossover | Tim Hardaway, Allen Iverson | The first crossover shifts the defender's balance twice as far. The usual source of broken ankles. | Give a step and keep him in front. A defender with first-class footwork. |
| Change of pace | Kyrie Irving | Hesitation moves freeze the defender: a short stun before the drive. | Stay off his hip, send him to his weaker hand. |
| Long-stride gather | Giannis Antetokounmpo | Picks the ball up near the free-throw line and still reaches the rim: the last help defender has less time to arrive. | Build a wall: three defenders in the lane before he starts. |
| Side step | Manu Ginobili | A two-step change of direction at the rim that takes him around a set defender. Charges against him are rarer. | Stay vertical and make him finish over length. |
| Spin move | Earl Monroe | A reverse pivot off the dribble that beats a defender who has cut off the drive. | A second defender waiting on the spin side. Strip it as he turns his back. |
| Wrong-foot finish | Steve Nash | Layups off either foot and either hand with no gather step: blocks against him are rare. | Body contact before the shot. |
| Power spin | Shaquille O'Neal | A drop step and spin with weight behind it: the post defender is moved, not beaten. Needs mass. | A very heavy defender, or foul him if he cannot shoot free throws. |
| Up and under | Kevin McHale | A shot fake in the post that gets the defender in the air, then a step through. Draws fouls. | A disciplined defender who stays down. |
| Post footwork | Hakeem Olajuwon | Chains of fakes and pivots: each fake the defender bites on makes the next one stronger. | Do not play him one on one. Double on the catch. |
| Jab step | Carmelo Anthony | From a standing start, a jab that moves the defender's balance before any dribble. | Crowd him so there is no room to jab. |

### Passing

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| No-look pass | Magic Johnson | His passes are read late: interception chance is cut, and the receiver's defender reacts late. | Play the passing lanes by position, not by his eyes. |
| One-handed sling | Nikola Jokic | Full-speed one-handed passes off the dribble or the rebound, with no gather. Opens passes others cannot throw in time. | Deny the cutters. Make him a scorer. |
| Full-court outlet | Wes Unseld, Kevin Love | After a defensive rebound, a long pass that starts a fast break before the defense is back. | Send two back on every shot. Do not crash the boards. |
| Touch pass | Larry Bird | Moves the ball on without catching it. The defense has no time to rotate to the next man. | Stay home on shooters. Do not help off the next pass. |
| Lob thrower | Chris Paul | Lobs to a rolling big man arrive where only he can catch them. | Drop the big man early and tag the roller from the weak side. |
| Pass fake | Rajon Rondo | A ball fake that moves a help defender out of the lane before the real pass or drive. | Help defenders with discipline. |
| Big man as hub | Arvydas Sabonis, Marc Gasol, Domantas Sabonis | Handoffs and passes from the high post: cutters around him get better looks, and a handoff works as a screen. | Sag off him if he cannot shoot. Deny the handoff. |
| Short-roll playmaker | Draymond Green | Catches the ball in the middle after a trap on the ball handler and makes the right four-on-three read. | Do not trap the ball handler. |

### At the rim

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| Poster dunk | Vince Carter, Blake Griffin | Dunks through contact. A defender in the way gets dunked on: a hit to his focus, a lift for the home crowd. Needs leap. | Take the charge early. Foul before he leaves the floor. |
| Lob finisher | DeAndre Jordan, Shawn Kemp | Catches and finishes anything thrown near the rim. A constant threat rolling to the basket. | A body on him before he rolls. |
| Putback instinct | Moses Malone | Rebounds his own miss and his team's misses near the rim far more than his rating says. | Box him out with two. |
| Floater or lob | Trae Young | In the lane with a big man waiting, the shot and the lob look the same until the release: the big man guesses. | Stay between the two and make someone else make the play. |

### Drawing fouls

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| Rip through | Kevin Durant | Swings the ball through a reaching defender's arms to draw a foul. | Hands back. Do not reach. |
| Pump fake | Dwyane Wade, DeMar DeRozan | Gets defenders in the air and jumps into them. Shooting fouls on jumpers. | Defenders who stay down. |
| Sells contact | Vlade Divac | Wins calls that were not there. With a tight whistle it pays. With a crew that lets them play, he is on the floor and play goes on. | A referee who is not fooled. |

### Defense

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| Chase-down block | LeBron James | Blocks fast-break layups from behind. A lost ball is not an automatic two points. | Finish with a dunk, or pull up. |
| Straight up | Roy Hibbert | Jumps vertically and takes contact without fouling. Shots at the rim are harder and he stays out of foul trouble. | Pull him away from the basket with a shooting big man. |
| Shot-blocking presence | Dikembe Mutombo | Attackers who have been blocked by him hesitate: fewer drives at him for a while. | Mid-range shots and floaters. |
| Takes the charge | Kyle Lowry, Ersan Ilyasova | Gets set in the driver's path early and often. Draws several times the usual number of charges. | Pull-up jumpers. Side steps. |
| On-ball pest | Gary Payton | Full-court pressure on the ball handler without fouling: costs him seconds and energy every possession. | Let someone else bring the ball up. |
| Big hands | Kawhi Leonard | Deflects passes and strips drivers at arm's length. Steals without gambling out of position. | Keep the ball away from his side of the floor. |
| Strip on the gather | Chris Paul | Takes the ball from big men as they bring it down to gather. | Big men who keep the ball high. |

### Screens and boards

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| Brick-wall screens | Steven Adams | Defenders who hit his screens are held up longer, legally. | Go under, or switch. |
| Seal for the driver | Marcin Gortat | After a ball screen he turns and screens the help defender, opening the lane. | Help from the other side. |
| Reads the bounce | Dennis Rodman | Gets to where a miss will land before anyone else. Rebounds far above his size. | Two men boxing him out, which frees a teammate. |
| Boxes out for others | Steven Adams | Takes two rebounders out of the play so a teammate gets the ball. His own numbers stay low. | Nothing direct. Crash from the perimeter. |

### Moments

| Move | Modelled on | In the simulation | Answer |
|---|---|---|---|
| Big-shot taker | Robert Horry | No drop in shooting in the last two minutes of a close game, when most players' composure is tested. | Deny him the ball late. |

## 4. What the engine needs first

Most of these cannot be built on today's engine, which decides contact situations with dice. They depend on work already under way or queued:

- **Bodies and contact** (in progress): power spin, brick-wall screens, the seal, boxing out, poster dunk, straight up, takes the charge.
- **Kinds of foul and the referee setting** (in progress): rip through, pump fake, sells contact, up and under, side step.
- **Dribble moves and defender balance** (queued, with a new Footwork attribute): crossover, change of pace, spin, jab step, post footwork, step-back three.
- **Shot variants**: the engine has three kinds of shot (rim, mid, three) and one contest rule. Hooks, fadeaways, floaters, bank shots and deep threes each need their own release and their own contest rule.
- **Pass variants and reads**: passes are one kind today. No-look, sling, outlet, touch, lob and pass fake need pass types with their own speed, risk and effect on the receiver's defender.
- **Per-frame action tags** (in progress): every move has to be recorded so the film room can list it and the 3D view can show it.

## 5. Open questions

- Should a player ever be able to learn a move from a teammate who has it (a veteran big man teaching post footwork)? The brief says no: signatures are innate. A weaker, learnable version of some moves could exist as ordinary skill.
- How visible should the counter be? Proposal: the assistant names it in the pre-game report once the opponent's signature is known.
- A research pass against real sources would extend this list and correct anything misremembered.
