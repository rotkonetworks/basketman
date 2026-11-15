# Coaching literature for the match engine

Compiled 2026-11-14. Purpose: give the 2D match engine positioning and movement rules that come from real coaching material, so simulated teams move like coached teams.

## 1. How to use this

- **Section 2** is the annotated bibliography. Every source has a short ref in square brackets, e.g. `[WABC-L2]`. Each entry says whether the URL was checked and how.
- **Section 3** distils the sources into rules a simulation can encode, one subsection per topic. Each rule carries the short ref (and page or section where known) it came from.
- **Section 4** lists the breakdowns coaches name for each defence, for the "mistake" log and match commentary.
- **Section 5** lists what is thinly sourced and which URLs could not be verified.

Conventions:

- Diagram numbering follows the sources: offence 1-5 (1 point guard ... 5 centre), defenders x1-x5.
- "Split line" (FIBA/Australian term) = "help line" = the basket-to-basket line. "Line of ball" = the line through the ball parallel to the endlines.
- US sources give feet. Conversions used here: 1 ft = 0.3048 m; 12 ft = 3.7 m, 15 ft = 4.6 m, 16 ft = 4.9 m, 18 ft = 5.5 m.
- A rule tagged `[design]` is my own inference or unit conversion, not a statement from a source. A rule tagged `[unsourced]` is common coaching usage for which I did not find a citable source that loads.
- URL status: **verified** = fetched on 2026-11-14 and the content was read; **loads** = returned HTTP 200 but only skimmed or summarised; **unverified** = could not be fetched from here (reason given).
- Paraphrase only. Nothing below is a verbatim passage from a copyrighted book.

The single most useful source is the FIBA/WABC Coaches Manual (levels 2 and 3 are free PDFs with page numbers). Where a US coaching site and the WABC manual both cover a topic, prefer the WABC wording for a FIBA-rules game.

---

## 2. Annotated bibliography

### 2.1 Rules and federation curricula

**[FIBA-OBR]** FIBA, *Official Basketball Rules 2024: Basketball Rules & Basketball Equipment*, v1.0a, approved by the FIBA Central Board 26 April 2024, valid from 1 October 2024.
URL: https://assets.fiba.basketball/image/upload/documents-corporate-fiba-official-rules-2024-v10a.pdf (verified; the `-v10.pdf` name that search engines return is a 404). Index page: https://refereeing.fiba.basketball/en/rules (loads). Free.
Good for: every court dimension (Art. 2), game timing (Art. 8), 3/5/8/24-second rules (Art. 26-29).

**[FIBA-OBRI]** FIBA, *Official Basketball Rules 2024: Official Interpretations*, v1.0a.
URL: https://assets.fiba.basketball/image/upload/documents-corporate-fiba-official-rules-2024-obri-v10a.pdf (loads). Free.
Good for: edge cases of the shot-clock reset and no-charge semi-circle rules.

**[WABC-L1]** FIBA / World Association of Basketball Coaches, *Coaches Manual, Level 1*, (c) 2016 FIBA-WABC.
URL (French PDF, verified, 393 pp.): https://assets.fiba.basketball/image/upload/documents-corporate-wabc-coaching-manual-level-1-fr.pdf
The English Level 1 PDF could not be found at any FIBA URL. The English web edition lived at `wabc.fiba.com/manual/level-1/`, which did not resolve in DNS on the check date; an archived copy of the index is at https://web.archive.org/web/20250211024159/https://wabc.fiba.com/manual/level-1/ (loads). Section numbers are identical between the French PDF and the English web edition, so this document cites Level 1 by section number. Free.
Good for: the foundations of man-to-man team defence (Team ch. 1.1: distance from opponent, one pass away, flat triangle, denial vs open stance, moving on the pass, split-line help, help against penetration, helping the helper, transition defence, full-court man); close-out technique (Player 1.1.2); 5-out motion and pass-and-cut spacing (Team 2.1); fast-break fundamentals (Team 2.3); offensive rebounding and transition (Team 2.5.1).

**[WABC-L2]** FIBA / WABC, *Coaching Manual, Level 2*, (c) 2016 FIBA-WABC, 365 pp.
URL: https://assets.fiba.basketball/image/upload/documents-corporate-wabc-coaching-manual-level-2-eng.pdf (verified). Free.
Good for: help on back-door and flash cuts (p. 227); low-post help and double teams (pp. 228-230); on-ball screen coverages "push"/ice and "weak" (pp. 232-235); team rotations against pick and roll (pp. 238-240); 2-2-1 full-court trap (pp. 243-248); half-court trap (pp. 249-250); 2-3 / 2-1-2 zone slides with the "pick-up line" (pp. 251-257); receivers' principles and 4-out-1-in motion (pp. 281-287); numbered fast break and trailer (pp. 312-318); zone offence principles (pp. 322-329).

**[WABC-L3]** FIBA / WABC, *Coaching Manual, Level 3*, (c) 2016 FIBA-WABC.
URL: https://assets.fiba.basketball/image/upload/documents-corporate-wabc-coaching-manual-level-3-eng.pdf (verified). Free.
Good for: pack line defence with metric distance (pp. 81-86); a multi-coverage pick-and-roll scheme drawn from Dusko Ivanovic (pp. 94-99); 1-2-2, 1-1-3 and 1-3-1 match-up zones (pp. 103-129); post double teams (pp. 135-139); box-and-1 (p. 144); summaries of read and react, flex, shuffle, triangle, horns, Princeton, UCLA cut, zipper (pp. 157-176); pick and roll against a zone (p. 208); late-clock offence (p. 232).

**[WABC-INTRO]** FIBA / WABC, *Coaches Manual* (introduction, basic rules, glossary), 40 pp.
URL: https://assets.fiba.basketball/image/upload/documents-corporate-wabc-coaches-manual-eng.pdf (verified). Free.
Good for: the glossary and the diagram conventions used in all three levels. Mini-basketball volume: https://assets.fiba.basketball/image/upload/documents-corporate-wabc-coaching-manual-mini-basketball-eng.pdf (loads).

**[BA-CME]** Basketball Australia, *Coaching Made Easy: Team Offensive and Defensive Skills* (NCAS Club Coach resource manual; undated, text refers to 2004).
URL (club-hosted copy, verified): https://beachcombersbasketball.club/wp-content/uploads/2024/07/Coaching-Made-Easy-Team-Offensive-and-Defensive.pdf . Free. No copy on an official Basketball Australia domain was found.
Good for: five transition lanes and who runs them; a six-step transition-defence rule set; split line, jump to the ball, deny one pass away / help two passes away, the "one third of the distance toward the ball" help rule.

**[BA-ZB]** Basketball Australia, *Zone Buster Manual: a guide to implementing the "No Zone" rule in U12 and U14 competitions*, (c) BA Limited 2009 (states it may be freely reproduced).
URL (club-hosted copy, loads): https://www.willettonbasketball.com.au/wp-content/uploads/2022/03/Zone-Buster-Manual.pdf . Free.
Good for: an operational definition of what counts as legal help-side man-to-man versus a zone, i.e. how far a help defender may sag.

**[USAB]** USA Basketball, *Youth Development Guidebook* (2nd ed.) and its successor *The USA Basketball Coaching Guide for All Levels* (BookLogix, 2025, ISBN 978-1-66530-196-1; paid).
URLs: press release for the 2025 book https://usab.prowly.com/387429-usa-basketball-coaching-guidebook-now-available-in-paperback-e-book (loads); third-party copy of the older guidebook https://cloud.rampinteractive.com/visionbasketballacademy/files/LTAD%20USA%20Basketball.pdf (loads, not an official host, content not read). No official usab.com download page was found.
Good for: a four-level player-development curriculum. Not used for any rule below.

**[BE-PDF]** Basketball England, *Player Development Framework Playbook*.
URL: https://www.basketballengland.co.uk/media/7138/the-pdf-playbook.pdf (**unverified**: returns HTTP 403 to automated fetches; exists in search indexes). Not used for any rule below.

### 2.2 Clinic notes and handouts

**[UW-PL]** "Washington Basketball: Pack Line Defense", clinic handout of the University of Washington women's basketball staff, file dated 14 September 2013 (author not named in the file).
URL: https://arkansasrazorbacks.com/pdf/wbasketball/nabes/UWPACKLINE9142013.pdf (verified). Free.
Good for: the most precise free statement of pack line positions: the pack line as one step inside the arc, the "gap" position, the "post" area, close-out technique, "guard your yard", shell drill calls.

**[HS-PL]** "Brad Soderberg and Dick Bennett Packline Defense Notes", Men's Basketball HoopScoop, 19 June 2013.
URL: https://www.mensbasketballhoopscoop.com/dick-bennett-and-brad-soderbergs-version-of-the-pack-line-defense/ (verified). Free.
Good for: Bennett's own priorities: pack line 16-17 ft from the basket, only the on-ball defender outside it, two guards back on every shot.

**[CT-BOE]** B. Williams, "Basketball Defense 2-3 Zone Part 2" (notes on Jim Boeheim's Syracuse zone, taken from Mike Neighbors' newsletter), Coaching Toolbox.
URL: https://coachingtoolbox.net/defense/basketball-defense-2-3-zone-part-2.html (verified). Free.
Good for: Syracuse short-corner and corner traps; guards fight over a screen on a shooter and go under on a non-shooter. It does not give the full slide table.

**[FIBA-ASSIST]** D. Ivanovic, pick-and-roll defence article, *FIBA Assist Magazine* no. 9 (2004). Existence confirmed only through the footnote in [WABC-L3] p. 94, which says its scheme is drawn from that article. **No working URL found** (the mirrors that search engines list return 404). Use [WABC-L3] pp. 94-99 instead.

### 2.3 Coaching sites (free articles)

Coach's Clipboard (J. Gels, coachesclipboard.net). All pages below were fetched and read; the diagrams are images, so the rules here come from the article text only. Some deeper material is members-only.

| Ref | Page | URL | Good for |
|---|---|---|---|
| [CC-M2M] | Man-to-man pressure defence | https://www.coachesclipboard.net/M2MDefense.html | deny / helpside / baseline-drive rotation |
| [CC-PL] | Pack line defence | https://www.coachesclipboard.net/BasketballPackLineDefense.html | pack line vs denial, post rules |
| [CC-23] | 2-3 zone | https://www.coachesclipboard.net/23zonedefense.html | slides, skip-pass coverage, traps |
| [CC-131] | 1-3-1 zone | https://www.coachesclipboard.net/131zonedefense.html | slides, corner trap, weak spots |
| [CC-122] | 1-2-2 / 3-2 zone | https://www.coachesclipboard.net/122zonedefense.html | slides, difference between the two |
| [CC-MU] | Match-up zone | https://www.coachesclipboard.net/MatchUpZone.html | matching rules, cutter rules |
| [CC-PNR-D] | Defending the pick and roll | https://www.coachesclipboard.net/BasketballDefensePicknRoll.html | rotations behind high and side screens |
| [CC-SW] | Switching defence | https://www.coachesclipboard.net/basketball-switching-defense.html | what to switch, mismatch costs, scram switch |
| [CC-221] | 2-2-1 press | https://www.coachesclipboard.net/221ZonePress.html | alignment, traps, interceptors |
| [CC-1211] | 1-2-1-1 press | https://www.coachesclipboard.net/1211ZonePress.html | first trap, two trappers / two interceptors / safety |
| [CC-PRESS] | Full-court man pressure | https://www.coachesclipboard.net/PressureDefense.html | pick-up rules, trap zones |
| [CC-RJ] | Run and jump | https://www.coachesclipboard.net/Run-and-Jump-Press.html | jump-switch vs trap |
| [CC-MOT] | Motion offence | https://www.coachesclipboard.net/MotionOffense.html | passer rules, post rules, hold time |
| [CC-4O1I] | 4-out 1-in motion | https://www.coachesclipboard.net/4Out1InMotionOffense.html | spots, post location rule |
| [CC-DDM] | Dribble drive motion | https://www.coachesclipboard.net/DribbleDriveMotionOffense.html | alignment, zones, reactions to drives |
| [CC-RR] | Read and react notes | https://www.coachesclipboard.net/ReadandReactOffenseNotes.html | layers 1-7 |
| [CC-PRIN] | Princeton offence | https://www.coachesclipboard.net/PrincetonOffense.html | sets, backdoor rule, centre as passer |
| [CC-TRI] | Triangle offence | https://www.coachesclipboard.net/TriangleOffense.html | spacing, options after each pass |
| [CC-FLEX] | Flex offence | https://www.coachesclipboard.net/FlexOffense.html | flex cut and screen-the-screener continuity |
| [CC-PNR-O] | Pick-and-roll offence | https://www.coachesclipboard.net/PickandRollOffense.html | where the other three stand |
| [CC-ZO] | Zone offence | https://www.coachesclipboard.net/ZoneOffense.html | front rule, overload, skip, screens |
| [CC-TD] | Transition defence | https://www.coachesclipboard.net/TransitionDefense.html | who gets back, priorities, 2v1, 3v2 |
| [CC-TO] | Transition offence | https://www.coachesclipboard.net/TransitionOffense.html | lanes, outlet, rim runner, trailer |
| [CC-ORTD] | Offensive rebounding and transition defence | https://www.coachesclipboard.net/offensive-rebounding-and-transtion-defense.html | crash/get-back options, roles |
| [CC-REB] | Rebounding | https://www.coachesclipboard.net/Rebounding.html | box-out, where misses go |

Other sites:

**[BFC-PL]** Basketball for Coaches, "Pack Line Defense: The Complete Guide". https://www.basketballforcoaches.com/pack-line-defense/ (verified). Pack line at 16 ft, no baseline, two back / three to the glass.
**[BFC-23]** Basketball for Coaches, "2-3 Zone Defense: The Complete Guide". https://www.basketballforcoaches.com/2-3-zone/ (verified). The clearest free slide description, including the forward "bump".
**[BFC-SHELL]** Basketball for Coaches, shell drill. https://www.basketballforcoaches.com/shell-drill/ (verified). 4v4 set-up and the three calls.
**[BFC-5OUT]** Basketball for Coaches, 5-out motion. https://www.basketballforcoaches.com/5-out-motion/ (verified). Spots and fill rules for each pass.
**[BFC-DROP]** Basketball for Coaches, drop coverage. https://www.basketballforcoaches.com/drop-coverage (verified). Drop roles and weaknesses.
**[HST-PNR]** Hoop Student, "Pick and roll defense". https://hoopstudent.com/pick-and-roll-defense/ (verified). One-paragraph definitions of over, under, hedge, ice, drop, switch, blitz, with the tag / low man.
**[LMH]** M. Campbell, "The Blueprint: Pick and Roll 101", Low Man Help (Substack), 18 Dec 2024. https://lowmanhelp.substack.com/p/the-blueprint-pick-and-roll-101 (verified, free). Low-man help and the X-out rotation.
**[SG-TAG]** Slappin' Glass, "'Attacking the Tag' Primer", 25 Nov 2020. https://slappinglass.substack.com/p/attacking-the-tag-concepts-and-actions (verified, free). Defines the tag and the offensive counters (shake / lift, 45 cut, Spain).
**[SG-TAGUP]** Slappin' Glass, "Tagging Up: A Primer", 2 May 2021. https://slappinglass.substack.com/p/tagging-up-a-primer (verified, free). Aaron Fearne's tagging-up rules.
**[SG-ICE]** Slappin' Glass, "Attacking ICE Coverage: A Primer", 23 May 2021. https://slappinglass.substack.com/p/attacking-ice-coverage-a-primer (loads; subscriber-gated). ICE description and counters.
**[FEARNE-POD]** The Basketball Podcast, ep. 45, "Aaron Fearne, Tagging Up Offensive Rebound System". https://the-basketball-podcast.captivate.fm/episode/episode-45-aaron-fearne-tagging-up-offensive-rebound-system (loads; audio, not transcribed here).
**[FIBA-TAG]** FIBA news feature on the New Zealand Tall Blacks and tagging up. https://www.fiba.basketball/en/events/fiba-basketball-world-cup-2027/news/fibas-tall-blacks-sparked-the-nba-rebounding-revolution (loads). Background on the spread of the system.
**[NFHS-TD]** NFHS, "The Keys to Transition Defense". https://nfhs.org/stories/the-keys-to-transition-defense (loads). Only one defender should jam the rebounder.
**[BB-RR]** Better Basketball (R. Torbett), Read & React Offense product site. https://www.betterbasketball.com/read-and-react-offense/ (loads; paid video series). The primary source for read and react; rules below come from [CC-RR] and [WABC-L3].

Could not be fetched (HTTP 403 to automated requests), therefore **unverified** and not used: Breakthrough Basketball pages on ice/drop/switch (`/defense/3-ball-screen-logalbo`), pick-and-roll coverages (`/fundamentals/defending-pick-and-roll-coverages`), 2-3 zone (`/defense/23-zone`), tagging up (`/defense/tagging-up-overview`); The Basketball Dictionary on Medium (`medium.com/the-basketball-dictionary/...`).

### 2.4 Books (paid; bibliographic records confirmed against Open Library, contents not consulted for this document)

- T. Winter, *The Triple-Post Offense*, Prentice-Hall, 1962. The original text of the triangle.
- D. Smith and B. Spear, *Basketball: Multiple Offense and Defense*, Prentice-Hall, 1982 (reissued Allyn & Bacon / Benjamin Cummings, 1998, ISBN 978-0-205-29119-9). Run-and-jump, scramble, passing game, secondary break.
- J. Wooden, *Practical Modern Basketball*, Ronald Press, 1966 (later editions Wiley / Allyn & Bacon). High-post offence, 2-2-1 press.
- D. Harris, *Winning Defense*, Masters Press, 1993, ISBN 978-0-940279-76-6. Professional man-to-man rotations.
- H. Brown, *Let's Talk Defense!*, McGraw-Hill, 2004, ISBN 978-0-07-144169-8. Pick-and-roll and post coverages at NBA level.
- P. Carril with D. White, *The Smart Take from the Strong*, Simon & Schuster, 1997 (University of Nebraska Press, 2004, ISBN 978-0-8032-6448-9). Princeton philosophy; little diagram detail.
- M. Wootten and J. Wootten, *Coaching Basketball Successfully*, 3rd ed., Human Kinetics, 2012, ISBN 978-0-7360-8372-0. A full programme manual.
- J. Krause, D. Meyer and J. Meyer, *Basketball Skills & Drills*, 3rd ed., Human Kinetics, 2007, ISBN 978-0-7360-6707-2. Individual and team fundamentals.
- G. Gandolfi (ed.), *NBA Coaches Playbook: Techniques, Tactics, and Teaching Points*, Human Kinetics, 2009, ISBN 978-0-7360-6355-5. Chapters by NBA coaches on pick and roll, transition, zone.
- D. Oliver, *Basketball on Paper*, Potomac Books, 2004, ISBN 978-1-57488-688-7. Not coaching, but the reference for possession-based evaluation (offensive-rebound value versus transition cost).

Left out because I could not confirm the bibliographic record: any Bob Knight book on motion offence, and Dick Bennett's pack line DVD notes.

---

## 3. Rules a simulation can encode

### 3.0 Shared vocabulary and court landmarks

Landmarks, FIBA court, origin at the centre of the endline, x along the endline, y toward mid-court `[FIBA-OBR Art. 2; derived values are [design]]`:

| Landmark | Position |
|---|---|
| Basket centre | (0, 1.575) |
| Backboard face | y = 1.20 |
| Low blocks (edge of key at the first rebound place) | (±2.45, about 1.75); the 1.75 m figure is read from Diagram 3, check the figure |
| Elbows (free-throw line meets key edge) | (±2.45, 5.80) |
| "Nail" (middle of the free-throw line) | (0, 5.80) |
| Short corner | about (±4.0, 1.0), midway between key edge and corner three `[design]` |
| Corner three | (±6.60 to ±6.75 from basket axis, y < 2.99) |
| Top of the arc | (0, 8.325) |
| Split line | x = 0 |

Distance-from-ball classes used by every man-to-man rule set: on ball; one pass away; two passes away; three passes away `[WABC-L1 Team 1.1.2-1.1.4]`.

### 3.1 Man-to-man defence

**On ball**
- Stay between ball and basket, close enough to pressure without being beaten; the defender is responsible for containing roughly one stride (about 0.9 m, "three feet") either side: "guard your yard" `[UW-PL]`.
- Do not get beaten to the outside (baseline from the wing, sideline from the top) and do not get beaten in a straight line to the rim; a straight-line drive cannot be helped `[UW-PL]`.
- Variant, pressure/denial man: force the ball to the sideline and then the baseline, where the low help defender meets it and traps `[CC-M2M]`. Variant, pack line: deny the baseline and send the dribbler to the middle, where the gap help is `[BFC-PL] [CC-PL]`. These are opposite forcing rules; make the forcing direction a team setting.
- If the dribbler has picked up the ball ("dead"), the on-ball defender closes up and every off-ball defender moves to deny `[WABC-L3 p. 81] [CC-PL]`.

**One pass away**
- Denial stance: back to the ball, chin on the shoulder nearest the opponent, the arm nearest the ball fully extended into the passing lane, thumb down; vision of both ball and opponent `[WABC-L1 Team 1.1.3, 1.1.5]`.
- Open ("floating") stance: back to the endline, off the opponent toward the ball; this allows the pass but gives more help on the dribbler `[WABC-L1 Team 1.1.3]`.
- Common mixed rule: deny on the ball side of the split line, float on the far side `[WABC-L1 Team 1.1.3]`.
- With the ball above the free-throw line extended, denying or allowing the guard-to-wing pass is a coach's choice; the pass back to the point is denied in pressure man `[WABC-L1 Team 1.1.3] [CC-M2M]`.
- Pack line version of one pass away: about halfway between ball and own player, one step off the line joining them, both feet inside the pack line ("gap") `[UW-PL] [WABC-L3 p. 81]`.

**Two or more passes away ("help side", "ball-you-man", flat triangle)**
- Open stance, see ball and opponent; the further the opponent is from the ball, the further the defender sags from the opponent `[WABC-L1 Team 1.1.4, 1.1.7]`.
- When the ball is on one side, defenders of players on the other side move to the split line `[WABC-L1 Team 1.1.7] [BA-CME p. 63]`. US phrasing: one foot on the help line when the ball is below the free-throw line `[CC-M2M]`.
- Numeric variant: stand one third of the distance from own player toward the ball; be on the split line if both ball and own player are below the free-throw line `[BA-CME p. 65]`.
- Two usual split-line spots: near the basket (low) and near the free-throw line (high) `[BA-CME, glossary]`.
- Treat the split line as a strip the width of the backboard (about 1.8 m), not a point; defenders shift within it: one step toward the ball when a drive is anticipated, one step toward own player when a pass is anticipated `[WABC-L1 Team 1.1.7]`.
- Ball at the top-middle: no help side exists, everyone plays straight up; ball in the corner: all five load to that half `[CC-M2M]`.

**Moving on the pass**
- All five move while the ball is in the air and are in the new position when it is caught ("jump to the ball", 1-3 steps toward the pass) `[BA-CME p. 64] [UW-PL] [WABC-L2 p. 255]`.

**Close-out**
- Sprint for the first part, then short choppy steps to arrive balanced; hands high on arrival to take away the quick shot and quick pass, leaving the drive as the option the defender is set for `[UW-PL] [WABC-L1 Player 1.1.2]`.
- Never give up the baseline on a close-out in pack line `[BFC-PL]`.
- Encode as: close-out time = distance / sprint speed + a fixed "chop" deceleration; a close-out that arrives without decelerating is beaten by a drive; one that arrives late concedes the shot `[design]`.

**Help and recover (stunt, hedge)**
- The nearest perimeter defender takes one or two steps at the dribbler with both feet, without reaching, and returns to his own player when the ball is picked up or passed `[WABC-L1 Team 1.1.8] [CC-M2M]`.
- Help is effective only from below the line of ball; a defender above the ball arrives at the dribbler's side and cannot stop him `[WABC-L1 Team 1.1.8] [WABC-L2 p. 238]`.
- Do not stunt off a dangerous shooter `[CC-M2M]`.

**Rotations**
- Help the helper: when a split-line defender commits to the ball, the next defender takes the helper's player until he recovers. It is not a switch; each defender still owns his original player `[WABC-L1 Team 1.1.9]`.
- Baseline drive from the wing: ball-side low defender steps to the baseline and stops or traps the ball with the on-ball defender; the weak-side low defender takes the vacated low player; the weak-side wing drops to the weak-side block ("sink"); the remaining top defender splits the two weak-side perimeter players and takes the skip pass ("fill") `[CC-M2M]`.
- Back-door cut: the lowest split-line defender takes the cutter or the pass; the next split-line defender drops to the basket; the top defender drops into the key `[WABC-L2 p. 227]`.
- Flash cut from the weak side: bump the cutter and stay between cutter and ball `[BA-CME p. 65]`; a floating one-pass-away defender can also help on it `[WABC-L2 p. 227]`.
- Middle drive: the first help comes from the nearest perimeter defender below the ball, not from the post defender `[CC-M2M] [WABC-L2 p. 238]`.

**Low-post defence and help**
- Post defended from behind: help comes from the perimeter in front (ball pressure plus a floating perimeter defender); on the catch the nearest perimeter defender either doubles or helps and recovers with his back to the baseline `[WABC-L2 pp. 228-229]`.
- Post fronted: the lob is covered by the low split-line defender; the next split-line defender rotates to the basket; the top drops into the key `[WABC-L2 p. 229]`.
- Double team source: if the post defender is on the high side the double comes from the low split-line defender; if he is on the baseline side it comes from the high split-line defender; the top defender zones the two players left `[WABC-L2 pp. 229-230]`.
- Variant: three-quarter front from the high side, full front with the ball in the corner `[CC-PL] [WABC-L3 p. 82]`.

**Shell drill (use it as the test harness)**
- 4v4, two guards (slots) and two wings. Ball in a slot: one on ball, two one pass away, one on the split line. Ball on a wing: one on ball, one one pass away, two on the split line `[BFC-SHELL]`.
- Calls: "ball", "deny" or "gap", "help" `[BFC-SHELL] [UW-PL]`.
- A static shell (offence passes and holds about two seconds) is a good unit test: after each pass, every defender's position should match the rules above `[UW-PL] [design]`.

**Pack line (Dick Bennett; Tony Bennett) versus pressure/denial man**
- The pack line is an arc about 16 ft (4.9 m) from the basket `[WABC-L3 p. 81] [BFC-PL]`; Bennett's notes say 16-17 ft `[HS-PL]`; other coaches define it as one step, or about 2 ft, inside the three-point arc `[UW-PL] [CC-PL]`. `[design]` The 16 ft figure dates from a shorter US college arc; with the FIBA arc at 6.75 m, "one step inside the arc" gives about 5.8 m. Make the radius a parameter, default 4.9-5.8 m.
- Only the on-ball defender is outside the line; the other four have both feet inside it, halfway between ball and opponent, a step off the passing line `[HS-PL] [UW-PL] [WABC-L3 p. 81]`.
- The ball is pressured hard; the defence is not passive `[WABC-L3 p. 81] [CC-PL]`.
- Perimeter passes are conceded, every catch is closed out, penetration gaps are closed by position rather than by rotation ("positioning is your help") `[UW-PL]`.
- Post: no catch with two feet in the post area (a step outside the lane, from the block up to the second lane mark); three-quarter front from the high side; on a catch, drop behind between post and basket `[UW-PL] [WABC-L3 p. 82]`.
- No switching of screens as a default `[BFC-PL]`.
- Transition comes first: two guards back on every shot, the other three block out and then sprint to half court `[HS-PL] [BFC-PL]`.
- Differences from denial man, for the engine: (a) one-pass-away position is the gap, not the passing lane; (b) forcing direction is middle, not baseline; (c) help is a stunt-and-recover from a neighbour, not a rotation from the split line; (d) more close-outs, fewer steals, fewer back-door lay-ups, more open perimeter catches `[WABC-L3 p. 83] [CC-PL]`.
- Stated weaknesses: good perimeter shooting, constant long close-outs, no shot clock, playing from behind `[WABC-L3 p. 83] [CC-PL]`.

### 3.2 Zone defences

General zone rules `[WABC-L2 pp. 251, 255]`: the closest defender takes the ball; move on the release of the pass; arms up in passing lanes; see the ball and the player who may become yours; front any low post on the ball side; on a shot each defender finds someone to block out. For any zone the designer must answer: who takes the ball on each pass, who stops penetration, who helps the helper, who stops passes into the key, who blocks out whom.

**2-3 (and 2-1-2)**

Set-up: guards at the elbows or a step above, forwards a step outside each block, centre in the middle of the key `[BFC-23]`.

| Ball at | Ball-side guard | Weak-side guard | Ball-side forward | Centre | Weak-side forward |
|---|---|---|---|---|---|
| Top | takes the ball | sinks to deny the high post | hedges a step toward the wing | middle of key | block |
| Wing | takes the ball, shades it toward the baseline | ball-side elbow / denies high post | bumps the wing on the catch if the guard is late, then drops to the short corner or fronts the low post | ball-side, half-fronting or behind the low post | steps a few steps up the lane, reads the skip |
| Corner | sags into the passing lane to the post, or denies the pass back out | high post / ball-side elbow | takes the ball | fronts the ball-side low post | to the middle of the lane, weak-side rebound, reads the skip |
| High post | pinch in, ready to recover out | pinch in | pinch toward the lane, ready to run to the corner | steps up to the ball | pinch toward the lane |
| Low post `[design]` | dig from the wing | high post | behind or fronting, with the centre | on the post | weak-side block |

Sources for the table: `[BFC-23]`, `[CC-23]`, `[WABC-L2 pp. 252-254, 257]`. The low-post row is not described directly by any of them.

- "Pick-up line": the coach fixes a line; a perimeter player with the ball above it belongs to a guard, below it to a forward `[WABC-L2 p. 252]`. Make this a parameter (default: free-throw line extended `[design]`).
- First pass to the wing, two variants: the forward closes out and is "bumped" down when the guard arrives `[BFC-23] [CC-23] [WABC-L2 p. 252]`; or the guard takes it directly. When the forward goes out, the centre slides toward the low post to help the helper `[WABC-L2 p. 252]`.
- Pass back out from the wing: if a forward was on the wing, the near guard takes the top; if a guard was on the wing, the other guard takes the top and the guards swap sides `[WABC-L2 p. 254]`.
- Skip wing to wing: the weak-side forward goes out until the guard arrives, then drops; skip wing to opposite corner: the forward on that side `[CC-23]`.
- Penetration: closest defender stops it; if a guard helps, the other guard rotates to that guard's wing and pushes the forward back down `[WABC-L2 p. 256]`.
- Never let the point split the two guards `[CC-23]`.
- Syracuse details: guards chase over a screen on a shooter and go under on a non-shooter; corner and short-corner traps are optional calls; the zone stays on when trailing by 8-10 `[CT-BOE]`.
- Trap options: wing trap (ball-side guard and forward), corner trap (forward and ball-side guard, the other guard denies the wing, weak-side forward covers the elbow) `[CC-23]`.
- Weak spots: high post / free-throw line; the wings between guard and forward; the gap at the top between the guards; skip passes and quick reversal that outrun the bump; the short corner dragging the centre or forward; rebounding without set match-ups `[BFC-23] [CC-23] [WABC-L2 pp. 322-323]`.

**1-3-1**

Set-up: x1 at the top, x2 and x3 on the wings at about the free-throw line extended, x5 in the middle at the high post, x4 on the baseline (roles from `[CC-131]`; the starting spots are inferred from its slides).
- Ball at the top: x1 pressures and pushes the ball to one side `[CC-131]`.
- Ball on the wing: ball-side wing takes the ball; x1 drops toward the ball-side elbow / high post; the weak-side wing drops to the weak-side block; x5 fronts or denies the post; x4 moves halfway to the ball-side corner, outside the lane `[CC-131]`.
- Ball in the corner: x4 closes out; the ball-side wing either drops in or traps with x4; x1 drops to the ball-side elbow; x5 denies the ball-side post; the weak-side wing holds the weak-side block `[CC-131]`.
- Reversal: x1 follows the ball and holds it until the far wing arrives; the old ball-side wing sprints to the weak-side block; x4 runs the baseline `[CC-131]`.
- Trap spots: corners (base version), wings and corners (aggressive version) `[CC-131]`.
- Weak spots: both corners (x4 has to cover sideline to sideline), the low post behind x5, the weak-side block on a skip, defensive rebounding with one player low `[CC-131]`. The offence should answer with a two-guard front `[CC-ZO]`.

**1-2-2 and 3-2**

Set-up: point at the top, two wings at the elbows extended, two low defenders at the blocks `[CC-122]`.
- Wing ball: ball-side wing takes it; the point drops to the ball-side elbow area to deny the high post; the ball-side low defender moves to mid-lane ready for the corner; weak-side wing sinks `[CC-122]`.
- Corner ball: ball-side low defender goes out; the other low defender crosses to the ball-side post; the ball-side wing denies the pass back out; the weak-side wing drops to the weak-side block `[CC-122]`.
- 3-2 difference: the point starts lower, at the free-throw line, and the wings wider; the high post is easier to cover and the top is more open; on a corner ball the weak-side wing goes to the ball-side elbow rather than the weak-side block `[CC-122]`.
- Weak spots: high post, corners, corner-to-post pass, weak-side block and rebound `[CC-122]`.

**Match-up zone**

- Start in a zone shape, then each defender takes the player in his area, so the shape copies the offence's alignment (for example, a 2-3 against a 1-3-1 offence becomes one on the point, two on the wings) `[CC-MU]`.
- Ball rule: on-ball defender plays tight man-to-man; neighbours deny or gap; far side helps `[CC-MU]`.
- Cutter rule (Jim Huber, as reported): a cutter going away from the ball is released to the next defender; a cutter going toward the ball is kept man-to-man until he can be handed off `[CC-MU]`.
- Simplest variant: after the first match-up, play man-to-man rules for the rest of the possession `[CC-MU]`.
- FIBA manual versions: 1-2-2, 1-1-3 and 1-3-1 match-ups, drilled separately for the guards and the back line `[WABC-L3 pp. 103-129]`.
- Weak spots: hand-off errors on cutters, overloads that put two players in one defender's area, screens on the perimeter of the zone `[CC-MU] [WABC-L2 p. 257]`.

### 3.3 Pick-and-roll coverages

Names: x1 guards the ball handler, x5 guards the screener. "Level" = the y-coordinate of the screen.

| Coverage | x1 (on ball) | x5 (screener's defender) | Behind the play |
|---|---|---|---|
| Over | chases over the top of the screen | varies with the coverage | none required `[HST-PNR]` |
| Under | goes under screener (and x5) and meets the ball on the other side | steps back to let x1 through, or stays attached ("hug") so x1 goes behind both | none; concedes the pull-up three; the screener may re-screen `[HST-PNR] [CC-PNR-D]` |
| Drop | over the top, pursues from behind and gets back in front | below the level of the screen, between ball and basket, keeps both ball and roller in front until x1 recovers | weak side stays home; low man tags a roller who gets behind x5 `[BFC-DROP] [HST-PNR] [LMH]` |
| Show / soft hedge | over, passing under the hedging big | steps out level with or above the screen to turn the ball, then sprints back to the roller | low man steps in to hold the roller until x5 is back; others sag one step `[HST-PNR] [WABC-L3 pp. 94-99]` |
| Hard hedge | over | steps out hard, perpendicular to the dribbler's path, for one or two dribbles | as show, with an earlier tag; the far perimeter defenders zone two players `[WABC-L3 pp. 94-99] [LMH]` |
| Blitz / trap ("double") | traps | traps and stays until the ball is passed or picked up | nearest defender denies the release pass; a split-line defender takes the roller; the last defender zones the two weak-side players `[WABC-L2 pp. 233, 239] [HST-PNR] [CC-PNR-D]` |
| Ice / down / push (side screens) | jumps to the high side of the ball, between ball and screen, and sends the dribbler to the baseline | below the screen toward the baseline, contains the drive | weak side stunts into gaps without a full rotation `[WABC-L2 pp. 232-233] [HST-PNR] [SG-ICE]` |
| Weak | forces the ball to the handler's weak hand whichever side the screen is on; goes over if that is toward the screen | steps off into the path on the forced side | the perimeter defender on the forced side stunts and recovers `[WABC-L2 pp. 234-235]` |
| Switch | takes the screener | takes the ball, stays up to deny the pull-up | help side loads toward the mismatch (see 3.4) `[HST-PNR] [CC-SW]` |
| Next | hands the ball to the next perimeter defender, who jumps to the ball, and rotates to that defender's player | stays with the roller | a perimeter rotation instead of a big-to-ball coverage `[unsourced]`; the closest sourced relative is the run-and-jump switch `[CC-RJ]` |

Rules common to all coverages:
- The two defenders in the action must use the same coverage; a mismatch between x1 and x5 (for example x1 icing while x5 shows high) is the standard source of a free run to the rim `[WABC-L2 p. 238] [LMH]`.
- Low man: the lowest weak-side defender owns the roller once the ball is past the screen; he comes from the weak side when the ball is driven away from him `[LMH]`. The tag is a bump on the roller, usually near the nail or in the lane, followed by a recovery to his own player `[SG-TAG] [HST-PNR]`.
- X-out with two weak-side defenders: low man meets the roller; the upper weak-side defender sinks to split two; on a skip to the corner the upper defender closes out to the corner and the low man closes out to the upper defender's player; on a pass to the wing both return to their own `[LMH]`.
- A corner left empty on the weak side removes the natural low man; help then has to come from the ball side, which favours the offence `[LMH]`.
- Middle screen, FIBA manual rotation: x5 pressures the ball, 5 dives; the nearest wing defender (x2) takes the dive; the far wing defender (x3) covers the first pass; when the ball is passed out x5 sprints either to his own player or to the open perimeter player, and x2 does not leave the roller until x5 has arrived `[WABC-L2 pp. 239-240]`.
- Help on the dribbler comes from below the ball; help on the roller lets x5 be more aggressive on the ball `[WABC-L2 p. 238]`.
- Choosing by personnel `[WABC-L3 pp. 94-99]`: good shooter on the ball, x1 goes over; poor shooter, x1 goes under. Screen set outside the three-point line, x5 shows hard and recovers. Screener who cannot shoot, switching is acceptable and x1 fights for low-post position; if the post then catches, trap him. Defenders of shooters do not help. A big who cannot contain the ball does not switch.
- Short shot clock: switch regardless of size `[CC-PNR-D]`.
- Coverage weaknesses: drop gives the pull-up and floater at about 4.5 m and the pick-and-pop `[BFC-DROP]`; hedge gives the short roll, the pop and the split between the two defenders `[HST-PNR]`; ice gives the pop and the mid-range pull-up `[HST-PNR]`; blitz gives 4-on-3 behind the trap `[HST-PNR]`; under gives the three `[HST-PNR]`; switch gives the mismatch `[HST-PNR]`.

### 3.4 Switching defences and their costs

- Scope options: switch everything (five similar defenders); switch 1-4 with the centre staying inside; switch by situation (last 5-10 seconds of the shot clock, after time-outs, wing ball screens only) `[CC-SW]`.
- By screen type: lateral and flare screens on the perimeter are switched; down screens are chased unless the two defenders are the same size; back screens are switched by the screener's defender calling it; post-to-post cross screens are switched `[CC-SW]`.
- Technique: call it before contact; switch tight with no gap between the two defenders; the new on-ball defender stays up `[CC-SW]`.
- Costs to model: a guard defending a big in the post and on the defensive glass; a big defending a guard on the perimeter; defenders relaxing after the switch; the offence hunting the weakest defender `[CC-SW] [HST-PNR]`.
- Repairs: active help side and denial of the post entry; switch back on the next perimeter pass; "scram" switch, where a nearby big takes the post while the entry pass is in the air and the guard takes the big's player `[CC-SW]`; fronting the post with help behind `[WABC-L3 pp. 94-99]`; trapping the post on the catch `[WABC-L3 pp. 94-99]`.
- Off-ball screens in the FIBA manual: lock and trail, under, through, switch are taught as four separate techniques; staggered screens are defended by three coordinated defenders with a switch option by the second screener's defender `[WABC-L1 Team 1.2.1-1.2.4] [WABC-L2 pp. 236-237]`.

### 3.5 Presses

Shared trap rules: two trappers, two interceptors, one safety `[CC-1211]`. Trap a dribbler who has turned, stopped or lost control, not a controlled dribbler and not the pass `[WABC-L2 p. 245] [CC-RJ]`. Trap zones: just over half court (the offence cannot go back), the backcourt sideline and corner (8-second pressure in FIBA), the frontcourt corners `[CC-PRESS]; 8 seconds per [FIBA-OBR Art. 28]`. Once the ball is ahead of a defender he sprints to get below the line of ball; if the press is beaten everyone except the on-ball defender runs to the key `[WABC-L2 p. 246] [CC-221] [CC-PRESS]`.

**Full-court man-to-man**
- After a score or dead ball each defender takes his own player; after a live turnover or rebound, the nearest `[CC-PRESS]`.
- On ball: force the dribbler to the sideline and try to turn him; never give the middle `[CC-PRESS] [CC-RJ]`.
- Ball side: deny in the passing lane; help side: sag to the middle and back `[CC-PRESS]`.
- Run and jump: a help defender ahead of the ball jumps at a dribbler coming toward him; the original on-ball defender rotates to the jumper's player. Jump-switch in the middle of the court, trap on the sideline `[CC-RJ]`.
- How offences beat it: clear four players to the frontcourt to isolate the dribbler, or let a forward bring the ball up against a slower defender `[WABC-L2 p. 243]`.

**2-2-1 (three-quarter court)**
- Alignment: two front defenders at about the free-throw line extended in the backcourt, two middle defenders behind them near half court, one safety `[CC-221] [WABC-L2 p. 244]`.
- The inbounds pass is allowed, toward a sideline and deep; the middle is denied `[WABC-L2 p. 244] [CC-221]`.
- After the first pass: ball-side front takes the ball and contains; the other front drops to the middle on the line of ball and must not be face-cut; the ball-side middle sits in the sideline passing lane; the other middle takes the centre of the court; the safety shades to the ball side `[WABC-L2 p. 245]`.
- Sideline trap: the front defender channels the dribbler to the ball-side middle defender, who traps only if the dribbler is out of control; the safety takes the sideline pass, the weak-side middle drops to the basket `[WABC-L2 pp. 245-246]`.
- Middle trap: if the dribbler is turned back to the middle, the other front defender traps or jump-switches; the weak-side middle takes the cross-court lane, the safety protects the basket `[WABC-L2 p. 246]`.
- Sideline pass completed: ball-side middle and safety trap the receiver; weak-side middle becomes the basket protector; both fronts sprint below the ball `[WABC-L2 p. 246]`.
- Reversal: the weak-side front takes the ball, the other front sprints to the split line, the middles swap roles `[WABC-L2 pp. 247-248]`. Backward passes are allowed because the 8-second count keeps running `[CC-221]`.

**1-2-1-1 (diamond)**
- Alignment: one on the inbounder, two wings at about the free-throw line extended, one at mid-court, one safety `[unsourced]` (the standard diamond; the free part of `[CC-1211]` describes the roles but not the starting spots).
- The on-ball defender shades the inbounder to force the pass to a corner; the first trap is on that receiver by the ball-side wing and the inbounder's defender, either immediately or after the first dribble (two calls) `[CC-1211]`.
- The far wing and the mid-court defender are the interceptors (middle and sideline); the safety stays back and leaves only for a certain interception `[CC-1211]`.
- Weak spot: the pass up the sideline; a team beaten there changes to a 1-2-2 `[CC-1211]`.

**Half-court trap**
- The on-ball defender makes the ball handler pass; middle receivers are denied and the sideline catch is allowed; the passer's defender (or the nearer wing if he is below the line of ball) traps the receiver; the next defender denies the sideline; two defenders sit on the split line reading the pass out; when the ball leaves the trap the defence drops into man or zone `[WABC-L2 pp. 249-250]`.

### 3.6 Offences

**Spacing numbers**
- Triangle: players 15-18 ft (4.6-5.5 m) apart `[WABC-L3 p. 169] [CC-TRI]`.
- Motion: 12-15 ft (3.7-4.6 m) apart `[CC-MOT]`. The "15-18 ft" figure the brief attributes to Bob Knight's motion could not be confirmed in a source I could open; see section 5.
- 5-out: spots on or a step behind the three-point line `[BFC-5OUT]`.
- Perimeter players stay outside the three-point line during a dribble entry `[BA-CME]`.
- `[design]` On the FIBA court five perimeter spots on the 6.75 m arc are about 4.5-5.5 m apart, which matches the 15-18 ft rule.

**Motion (Henry Iba; Bob Knight)**
- Sets: 3-out 2-in, 4-out 1-in, 5-out `[CC-MOT]`.
- After a pass the passer must do one of: basket cut, screen away, follow for a ball screen, V-cut and replace himself; after a skip pass, drift and look for the back cut; after a post entry, slide to the corner `[CC-MOT]`.
- Hold the ball for a count of two so cuts and screens can develop `[CC-MOT]`.
- A teammate dribbling at you means back-cut `[CC-MOT] [CC-4O1I]`.
- Denied on the wing: the post flashes high and the wing cuts back door `[CC-MOT]`.
- Guard rule in 4-out: any time a guard passes he screens away for the players on the other side `[WABC-L2 p. 285]`.
- Knight's version is defined by screening and by reading the defender; it has no fixed pattern `[CC-MOT]`.

**5-out and read and react (Rick Torbett)**
- Five spots: two corners, two wings, top `[BFC-5OUT]`.
- Pass and cut: after a pass one spot away, the passer cuts to the basket and fills the open corner; the others fill up toward the ball from the baseline side `[CC-RR] [BFC-5OUT]`. Pass top to wing or wing to corner: cut and go to the weak-side corner. Pass corner to wing: cut and replace self `[BFC-5OUT]`.
- Read line: if a one-pass-away defender has a foot above the three-point line (denying), cut back door `[CC-RR] [BFC-5OUT]`.
- Dribble-at: back cut; or hand-off if the dribbler turns his back (power dribble) `[CC-RR]`.
- Circle movement: when the ball is driven right, the other four rotate right (counter-clockwise seen from above); driven left, rotate left; the vacated spot behind the driver is filled as the safety `[CC-RR] [WABC-L3 pp. 157-159]`.
- Baseline drive exception: the driver must have four outlets: opposite corner (stays), 45-degree, 90-degree, and the safety behind him `[CC-RR] [WABC-L3 p. 159]`.
- Post reaction: drive from the top, post slides to the short corner; baseline drive, post steps up to the elbow `[CC-RR]`.

**4-out 1-in**
- Four perimeter players across six spots (two top, two wings, two corners); the post follows the ball: ball-side elbow when the ball is on top, ball-side block when it is on the low wing or in the corner; or stays on the weak-side block to open the drive `[CC-4O1I]`.
- Receivers' principles on a drive: drive to the middle, the low post steps to the basket, one shooter spots up opposite, one player becomes safety, one moves behind the dribbler; baseline drive, the low post circles out to the perimeter `[WABC-L2 pp. 281-282]`.
- Pressure releases: high-post catch with a back cut by the denied perimeter player; low post stepping to the short corner when reversal is denied `[WABC-L2 p. 282]`.

**Dribble drive motion (Vance Walberg; popularised by John Calipari)**
- Alignment: two guards in the slots, two shooters deep in the corners, the big on the weak-side block; the big does not post up `[CC-DDM]`.
- The corners create a double gap between slot and corner; a single gap is hard to drive `[CC-DDM]`.
- Zones along the drive: breakdown (beat your man), drop (around the free-throw line), drag (between there and the rim), rack (the rim). Target: a lay-up or a three `[CC-DDM]`. Exact zone boundaries differ between secondary sources.
- Slot drive down the lane line: the other guard slides behind the driver for the kick-back; the ball-side corner lifts to the high wing; the big stays weak side for the dump or the rebound; after passing, the driver clears to the opposite corner `[CC-DDM]`.
- Middle drive: the big moves to the opposite side of the lane; the opposite wing lifts; the other corner stays `[CC-DDM]`.
- Baseline drive from the wing: the big "T-ups" into the middle of the lane; the weak-side wing drops to the corner `[CC-DDM]`.

**Princeton (Pete Carril)**
- Two rules: your defender tells you where to go; if you are denied, cut back door `[WABC-L3 p. 172]`.
- Start: two-guard front, two forwards at the free-throw line extended, centre high or low; players stay at or above the free-throw line extended to keep the basket area empty `[WABC-L3 p. 172] [CC-PRIN]`.
- The centre at the high post or elbow is the passer; cutters play off him `[CC-PRIN]`.
- Base pattern in the FIBA summary: pass to the wing, both guards cut low and come back out off screens by the forwards (turn-out cuts) while the centre steps out to pass, then the centre sets a staggered screen away `[WABC-L3 pp. 172-173]`.
- Dribble-at means back cut `[CC-PRIN]`.

**Triangle (Sam Barry; Tex Winter)**
- Strong side: post on the block, wing, corner. Weak side: a guard at the top and a forward at the weak-side elbow area (two-man game) `[WABC-L3 p. 169] [CC-TRI]`.
- Spacing 15-18 ft so that one defender cannot guard two `[WABC-L3 p. 169]`.
- First look is the post. On a post entry the passer cuts first off the post and the other perimeter player follows (split cut) `[CC-TRI]`.
- Other outlets from the wing: corner (then cut through), top guard (post seals, re-feed), weak-side forward flashing; a reversal rebuilds the triangle on the other side `[CC-TRI]`.

**Flex**
- Baseline back screen by the ball-side block player for the corner player, who cuts along the baseline to the opposite block (flex cut); then the passer screens down for the back-screener, who comes to the elbow (screen the screener); the down-screener steps to the corner; the ball is reversed and the same action runs from the other side `[CC-FLEX] [WABC-L3 pp. 166-167]`.

**Pick-and-roll spacing**
- The ball handler takes at least two dribbles off the screen to pull the two defenders apart `[CC-PNR-O]`.
- Middle (spread) pick and roll: two shooters in the corners, the fourth player on the weak-side slot or elbow `[CC-PNR-O]`; with the second big in the corner, his defender is told not to help `[WABC-L3 pp. 94-99]`.
- Side pick and roll: the other three form a weak-side triangle `[CC-PNR-O]`.
- Roll to the rim, or pop to the elbow, slot or corner `[CC-PNR-O]`.
- Shake / lift: as the ball handler attacks, the weak-side corner player lifts up the arc toward the wing behind the roll, so the tagger must bump the roller and then make a long close-out `[SG-TAG]`.
- Other counters to the tag: 45 cut from the corner behind a nail stunt, baseline cut, second big in the dunker spot, Spain (back screen on the dropping big), empty corner `[SG-TAG] [LMH]`.
- Against a hard show the screener is forced to dive and the second high post is left open rather than the roller `[WABC-L3 pp. 94-99]`.

**Post entries and spacing around the post**
- Post position: on the edge of the key above the block, not inside it `[BA-CME]`.
- Do not feed the post from the top; feed from the wing or the corner `[BFC-PL]` (stated as the angle a defence wants to force).
- After the entry: passer cuts (triangle split) or slides to the corner (motion) `[CC-TRI] [CC-MOT]`.
- Fronted post: the opposite post flashes to the elbow for the high-low while the low post seals `[CC-MOT]`.
- Doubled post: pass out to the side the double came from; from the corner or baseline double, skip to the opposite wing `[CC-MOT] [CC-TRI] [CC-4O1I]`.
- Moving the help: cut the weak-side players to the strong side, or lift the weak-side big to the high post, to pull defenders off the split line `[WABC-L2 p. 230]`.
- `[design]` Keep the nearest perimeter teammate at least 4.5 m from the post so one defender cannot dig and recover.

**Isolation**
- Sourced only indirectly: a "4 flat" or cleared-side alignment, where the other four take their defenders away from the split line, is the same idea as "clearing the split line" for a post-up `[WABC-L2 p. 230]`; late-clock sets are in `[WABC-L3 p. 232]` (not extracted here).
- `[design]` Encode isolation as: ball at the top or wing; the four others on the arc on the far side or in both corners; no defender allowed a help position below the ball without leaving a shooter.

**Zone offence**
- Stand in the gaps between two defenders `[WABC-L2 p. 322]`.
- Front rule: against a two-guard front use a one-guard set (1-3-1, 1-4, 3-out 2-in); against a one-guard front use a two-guard set (2-1-2, 2-3, 4-out 1-in) `[CC-ZO]`.
- Posts play behind the zone and flash from behind into the gap at the free-throw line when the defender in front turns to the ball `[WABC-L2 pp. 322-323]`.
- Short corner: a post stepping there stretches the back line; rule for two posts: one gets the ball, the other goes to the basket `[WABC-L2 p. 323]`.
- Gap dribble: dribble between two defenders to make both commit, then pass to the player one of them left `[WABC-L2 pp. 324-325]`.
- Reverse the ball and skip it: corner to opposite wing, wing to opposite corner `[CC-ZO]`.
- Overload one side with more players than the zone has defenders there `[CC-ZO]`.
- Screen the outside of the zone (pin the weak-side low defender, skip to the corner) `[CC-ZO] [WABC-L2 pp. 327-329]`.
- Get a high- or low-post touch before an outside shot; go inside-out `[CC-ZO]`.
- Run before the zone is set, and send rebounders, because the zone has no fixed box-out assignments `[CC-ZO] [BA-CME]`.

### 3.7 Transition

**Offence**
- Five lanes: middle, two outside lanes, two inside lanes next to the middle `[BA-CME pp. 6-8]`.
- Numbered break: 1 takes the outlet and advances the ball (passing ahead if anyone is ahead); 2 and 3 run the two outside lanes as wide as possible; 4 (first big) sprints the inside lane to the rim; 5 (the rebounder or inbounder) trails `[WABC-L2 pp. 312-314]`.
- If the roles are not fixed: 4 and 5 are interchangeable (whoever rebounds trails), 2 and 3 take the nearest side, and if both are on one side the trailing one crosses `[WABC-L2 p. 314]`.
- Outlet: ball-side sideline, about level with the three-point line; after a steal, pass ahead on the sideline and not through the middle `[CC-TO]`.
- The ball goes to the middle on the dribble, with as few dribbles as possible (three for elite players) `[BA-CME p. 6]`.
- Wings: sprint to half court, look inside for the ball after half court `[BA-CME p. 7]`; finish below the free-throw line extended, in the corner or bouncing off the baseline back to the wing `[WABC-L2 p. 313] [CC-TO]`. Variant: outside lanes cut to the basket at the free-throw line extended on a 3v2 `[BA-CME]`.
- Trailer: stops at the top or weak-side slot for a three, a reversal or a ball screen `[WABC-L2 pp. 314-315] [CC-TO]`.
- 2v1: stay wide, attack along the lane line, pass when the defender commits. 3v2: ball in the middle, stops at the free-throw line; wings cut at 45 degrees `[CC-TO] [BA-CME]`.
- If nothing is open, flow into the secondary break and then the half-court set (4-out 1-in or 5-out) `[WABC-L2 p. 313]`.

**Defence**
- Transition defence starts when the shot goes up: each offensive player is either in the key to rebound or at the top of the key as a safety; not all five rebound `[WABC-L1 Team 1.1.11]`.
- "Basket and ball", in that order: nobody commits to the ball until the basket is covered `[WABC-L1 Team 1.1.11]`.
- Six-step rule set `[BA-CME ch. 2]`: (1) the player nearest the ball slows the dribbler and calls "ball"; (2) everyone else protects the basket first; (3) the first one back takes the basket and calls "basket"; (4) the second back takes the first pass; (5) the third back takes the second pass; (6) the fourth back helps the basket, then takes any free player.
- US phrasing of the same thing: first back to the rim and one step out; second back stops the ball wherever it is; the other three sprint back looking for shooters rather than running blindly to the paint (young teams: paint first, then match up) `[CC-TD]`.
- One rebounder pressures the defensive rebounder with arms up to delay the outlet, without reaching; only one `[WABC-L1 Team 1.1.11] [CC-TD] [NFHS-TD]`.
- Outnumbered: 1v2, retreat to the basket, fake at the ball, give up the jump shot and not the lay-up; 2v3, tandem: top defender stops the ball, bottom defender takes the first pass, top drops to the basket `[WABC-L1 Team 1.1.11] [CC-TD]`.
- "First three back" in the brief maps to steps 3-5 above. No source I opened uses that exact phrase.

### 3.8 Rebounding

**Defensive**
- On the shot: find the opponent, make contact, turn, seal him away from the basket, then go to the ball `[CC-REB] [WABC-L1 Player 1.2.4]`.
- Perimeter defenders check the shooter or their player with a forearm and then rebound from the elbow area if he retreats `[CC-REB]`.
- Rebounding triangle: one player each side of the basket and one in front `[CC-REB]`.
- Where misses go: about 70 % of misses from the wing or corner go long to the opposite side; missed threes rebound long, so two perimeter players should cover the wing-elbow areas `[CC-REB]` (percentage is quoted by the site without a study; treat as a tunable prior).
- Zone: nobody has a fixed opponent, so each defender blocks out whoever is in his area; this is the stated rebounding weakness of every zone `[WABC-L2 p. 255] [BFC-23]`.

**Offensive: how many crash**

| Scheme | Crash | Back | Source |
|---|---|---|---|
| Two guards back (Bennett / pack line teams) | 3 | 2, at half court either side of the centre circle; ball-side guard stops the ball, the other protects the rim | `[HS-PL] [BFC-PL] [CC-TD]` |
| Classic 3-1-1 | 3 (weak side, front, ball side) | point guard back as safety; 2 as half rebounder at the free-throw line / top of key | `[CC-MOT] [CC-TD]` |
| Four to the glass (Brad Underwood) | 4; two of them take the wing-elbow long-rebound areas | the three-point shooter is first back | `[CC-ORTD] [CC-REB]` |
| Tagging up (Aaron Fearne) | 5 | none designated | `[SG-TAGUP]` |

- Tagging up rules: on the shot every offensive player goes to the high side (the half-court side) of his own defender and pushes him toward the basket; on a miss rebounded by the defence each tagger is already matched up and stays with that player; the nearest takes the ball; ball-side players sprint back taking away straight-line passes; the others flood the middle. On a make, stay matched to pressure the inbound. The shooter stays with the defender who closed out on him `[SG-TAGUP]`.
- Tagging-up errors named by the source: the shooter not tagging the close-out defender; running past the match-up to chase the ball; going under (baseline side of) the block-out instead of to the high side `[SG-TAGUP]`.
- Costs of tagging up: cross-matches in transition, more offensive fouls, more effort `[SG-TAGUP]`.
- 50/50 rule: crash only if you have an even chance at the ball, otherwise sprint back; never get caught on the baseline side `[CC-ORTD]`.
- Trade-off: more crashers means more second chances and more exposure in transition; the site quotes an unnamed study that a get-back emphasis can halve offensive rebounding `[CC-ORTD]`. Use [Oliver, *Basketball on Paper*] for a possession-value treatment when tuning.

### 3.9 FIBA geometry and timing

All from `[FIBA-OBR]` 2024, Art. 2, 8, 26-29. Lines are 5 cm wide; note which edge each measurement uses.

| Item | Value | Article |
|---|---|---|
| Court | 28 m x 15 m, measured from the inner edge of the boundary lines | 2.1 |
| Basket centre | 1.575 m from the inner edge of the endline | 2.5.4 |
| Three-point arc | radius 6.75 m from the point under the basket centre to the outer edge of the arc | 2.5.4 |
| Corner three | straight lines parallel to the sidelines, outer edge 0.90 m from the inner edge of the sideline, so 6.60 m from the basket axis | 2.5.4 |
| Arc meets straight line | about 2.99 m from the endline (derived: 1.575 + sqrt(6.75^2 - 6.60^2)) | `[design]` |
| Top of arc | 8.325 m from the endline (derived; equals the throw-in line distance) | 2.5.4, 2.5.6 |
| Three-point line | the line itself is not part of the three-point area (a foot on the line is a two) | 2.5.4 |
| Restricted area (key) | rectangle 4.90 m wide (outer edges 2.45 m either side of the endline mid-point) by 5.80 m deep (far edge of the free-throw line); the lines are part of the area | 2.5.3 |
| Free-throw line | far edge 5.80 m from the endline, 3.60 m long; so 4.225 m from the basket centre (derived) | 2.5.3 |
| Free-throw semi-circle, centre circle | radius 1.80 m to the outer edge | 2.5.2 |
| No-charge semi-circle | radius **1.30 m to the outer edge** (1.25 m to the inner edge), closed by two 0.375 m lines ending 1.20 m from the endline, under the front of the backboard; the line is part of the no-charge area | 2.5.7 |
| Throw-in lines | 8.325 m from the endline | 2.5.6 |
| Game | 4 x 10 min, overtime 5 min | 8 |
| Shot clock | 24 s; reset to 14 s when the offence regains the ball after it touched the ring, and on most frontcourt resets when 13 s or fewer showed | 29 |
| Backcourt | 8 s to reach the frontcourt | 28 |
| Closely guarded | 5 s to pass, shoot or dribble | 27 |
| Key | 3 s | 26 |
| Team fouls | penalty after 4 team fouls in a quarter | 41 |

Correction to the brief: the no-charge semi-circle is 1.25 m only to the inner edge of its line. Since the line belongs to the no-charge area, the effective radius for a "defender is inside" test is 1.30 m.

Consequences for the engine `[design]`:
- The corner three is 15 cm shorter than the arc and leaves only 0.90 m between line and sideline; a corner shooter's spot is at most about 0.8 m deep.
- A pack line "16 ft" arc (4.9 m) passes about 0.7 m above the free-throw line at the top and almost exactly through the elbows; a "one step inside the arc" pack line (about 5.8 m) is noticeably wider on the FIBA court.
- The 14-second reset makes a second possession short, which raises the value of an immediate put-back or kick-out three after an offensive rebound.
- The 8-second rule is what makes backcourt traps and allowing backward passes worthwhile `[CC-221]` (US sources say 10 seconds; use 8).

---

## 4. Breakdowns coaches name (for the mistake log)

Each line: label, then what happened, then where the sources name it.

**Man-to-man (pressure / denial)**
- `ball_watching`: off-ball defender looks only at the ball and loses a cutter (back door or flash) `[WABC-L1 Team 1.1.4]`.
- `beaten_straight_line`: on-ball defender beaten in a straight line to the rim, no help possible `[UW-PL]`.
- `beaten_baseline` / `beaten_outside`: on-ball defender gives up the side the team does not help on `[UW-PL] [BFC-PL]`.
- `late_closeout`: arrives after the catch-and-shoot window `[WABC-L3 p. 83]`.
- `flyby_closeout`: closes out without chopping, beaten by the drive `[UW-PL] [CC-PL]`.
- `no_jump_to_ball`: stood still while the pass was in the air `[UW-PL] [BA-CME]`.
- `help_from_above`: help came from above the line of ball and did not stop the drive `[WABC-L1 Team 1.1.8]`.
- `no_help`: split-line defender not on the split line when the drive came `[CC-M2M]`.
- `no_help_the_helper`: first rotation made, second not; dump-off or lob to the helper's player `[WABC-L1 Team 1.1.9] [WABC-L2 p. 227]`.
- `slow_rotation`: baseline-drive rotation late `[CC-M2M]`.
- `reach_in`: helper or on-ball defender reached and lost balance or fouled `[WABC-L1 Team 1.1.8] [CC-M2M]`.
- `overdeny_backdoor`: denial too high, beaten back door `[WABC-L2 p. 227]`.
- `face_cut`: cutter got between defender and ball `[WABC-L2 p. 245] [UW-PL]`.
- `deep_post_catch`: post allowed to catch with two feet in the post area `[UW-PL]`.
- `helped_off_shooter`: stunted or rotated off a shooter `[CC-M2M] [WABC-L3 pp. 94-99]`.

**Pack line**
- `out_of_pack`: an off-ball defender outside the line, opening a driving gap `[HS-PL] [UW-PL]`.
- `gap_not_closed`: not halfway between ball and player; drive goes through the gap `[UW-PL]`.
- `late_closeout`, `flyby_closeout`, `beaten_baseline` as above; these are the named weak points of the scheme `[WABC-L3 p. 83] [BFC-PL]`.
- `gambled_for_steal`: left the gap to deny or steal with a live dribble `[BFC-PL]`.
- `no_dead_ball_pressure`: failed to deny when the dribble was dead `[CC-PL]`.

**Zones**
- `gap_split`: dribbler went between two zone defenders (most often the two guards of a 2-3) `[CC-23] [WABC-L2 p. 324]`.
- `high_post_open`: no one fronting or denying the high post `[BFC-23] [WABC-L2 p. 257]`.
- `late_skip_slide`: skip pass or quick reversal arrived before the slide or bump `[BFC-23] [CC-23]`.
- `no_bump`: forward stayed on the wing or guard never arrived, leaving the corner or block open `[BFC-23]`.
- `corner_open` / `short_corner_open`: baseline defender late (1-3-1, 1-2-2) or the 2-3 forward held high `[CC-131] [CC-122]`.
- `weak_side_block_open`: weak-side wing or forward did not drop `[CC-131]`.
- `screened_in`: outside zone defender pinned by a screen before the pass `[WABC-L2 p. 257]`.
- `flash_from_behind`: defender turned to the ball and lost the player behind him `[WABC-L2 p. 322]`.
- `no_box_out_zone`: no opponent found on the shot `[WABC-L2 p. 255] [BFC-23]`.
- `no_talk`: defenders moved differently from each other; the manual's point is that an unplanned slide is fine if communicated `[WABC-L2 p. 255]`.
- `cutter_not_passed_on` (match-up): cutter left free at a hand-off `[CC-MU]`.

**Pick-and-roll coverages**
- `coverage_mismatch`: the two defenders played different coverages `[WABC-L2 p. 238] [LMH]`.
- `rejected_screen`: ball handler refused the screen and drove the other way (ice not set, or drop defender on the wrong side) `[BFC-DROP]`.
- `split_the_hedge`: dribbler went between x1 and the hedging big `[HST-PNR]`.
- `slow_hedge_recovery`: hedging big late back to the roller or popper `[HST-PNR] [WABC-L3 pp. 94-99]`.
- `no_tag`: low man did not meet the roller `[LMH] [SG-TAG]`.
- `over_tag`: tagged a contained roller and gave up the corner or lifted three `[SG-TAG]`.
- `late_x_out`: skip to the weak side beat the rotation `[LMH]`.
- `left_roller_early`: the tagger went back before the big had recovered `[WABC-L2 p. 240]`.
- `under_on_shooter`: went under against a shooter `[WABC-L3 pp. 94-99] [HST-PNR]`.
- `drop_too_deep`: pull-up or floater conceded `[BFC-DROP]`.
- `pick_and_pop_open`: dropping or icing big did not get out to a shooting screener `[BFC-DROP] [HST-PNR]`.
- `trap_split` / `trap_released`: blitz let the ball out easily; 4-on-3 behind `[HST-PNR]`.

**Switching**
- `late_switch` / `soft_switch`: gap between the switching defenders, open shot or slip `[CC-SW]`.
- `no_switch_call`: two defenders on one player `[CC-SW]`.
- `mismatch_post`: small defender posted after a switch `[CC-SW] [HST-PNR]`.
- `mismatch_perimeter`: big beaten off the dribble after a switch `[HST-PNR]`.
- `mismatch_rebound`: guard boxed out by a big after a switch `[CC-SW]`.
- `relaxed_after_switch` `[CC-SW]`.

**Presses**
- `middle_open`: pass or dribble through the middle of the press `[CC-221] [CC-RJ]`.
- `face_cut` in the middle `[WABC-L2 p. 245]`.
- `trap_on_controlled_dribbler`: trapped a dribbler in control; split or passed out `[WABC-L2 p. 245]`.
- `trap_split`: dribbler went between the trappers `[CC-RJ]`.
- `reach_in_trap`: foul in the trap `[CC-PRESS]`.
- `sideline_pass_open`: interceptor not in the sideline lane (the named weak spot of the 1-2-1-1) `[CC-1211]`.
- `safety_gambled`: safety left for a steal and conceded a lay-up `[CC-221] [CC-1211]`.
- `not_below_ball`: beaten defenders did not sprint below the line of ball `[WABC-L2 p. 246]`.
- `late_reversal_rotation` `[WABC-L2 pp. 247-248]`.

**Transition defence**
- `no_safety`: all five went to the offensive glass `[WABC-L1 Team 1.1.11]`.
- `ball_before_basket`: last defender went to the ball and left the basket `[WABC-L1 Team 1.1.11]`.
- `ball_not_stopped` `[CC-TD] [BA-CME]`.
- `ran_to_paint_lost_shooter`: sprinted to the key and left a trailing or wing shooter `[CC-TD]`.
- `loafed_back`: jogged; Bennett substitutes the player `[BFC-PL]`.
- `two_jammed_rebounder` `[NFHS-TD]`.
- `no_talk`: no "ball" or "basket" call `[BA-CME ch. 2]`.
- `cross_match`: matched to the wrong size after tagging up `[SG-TAGUP]`.

**Rebounding**
- `no_box_out`, `pushed_under_rim`: sealed too deep, ball over the head `[CC-REB]`.
- `weak_side_unblocked`: long rebound to the far side lost `[CC-REB]`.
- `long_rebound_uncovered`: nobody at the elbows on a missed three `[CC-REB]`.
- `tag_low_side`: tagger went under the block-out `[SG-TAGUP]`.
- `shooter_not_tagged` `[SG-TAGUP]`.
- `crashed_without_chance`: broke the 50/50 rule and was caught behind the ball `[CC-ORTD]`.

---

## 5. Gaps and unverified items

Thinly sourced topics:
- **"Next" coverage** in the pick and roll: no citable source that loads; the row in 3.3 is `[unsourced]`.
- **Bob Knight's motion rules** and the "15-18 ft" spacing figure: the figure is confirmed for the triangle (`[WABC-L3 p. 169]`, `[CC-TRI]`), and `[CC-MOT]` gives 12-15 ft for motion. An ESPN.com article from 2003 that reportedly gives 15-18 ft for Knight-style motion (`https://www.espn.com/ncb/2003/0225/1514311.html`) returned an empty HTTP 202 and was not read. No Knight book or clinic note was confirmed.
- **Jim Boeheim's own slide rules**: only second-hand notes (`[CT-BOE]`) and generic 2-3 guides. His detailed teaching is on paid video.
- **Isolation spacing** and **post-entry spacing**: only indirect statements; several rules there are `[design]`.
- **3-2 zone** and **1-2-2 zone**: one source (`[CC-122]`), plus the 1-2-2 match-up in `[WABC-L3]`, whose slides were not extracted in detail.
- **1-3-1**: the free part of `[CC-131]` omits the ball-at-top and trapping detail; `[WABC-L3 pp. 126-129]` covers the match-up version and was not extracted in detail.
- **Drop depth, hedge distance, tag location in metres**: no source gives numbers; they give relative positions (below the level of the screen, at the nail).
- **Rebound direction percentages**: one unsourced "about 70 %" figure.
- **USA Basketball** and **Basketball England** curricula: located but not read; no rule depends on them.
- **Dribble drive zone boundaries**: secondary sources disagree; no Walberg primary document was read.

URLs that could not be verified:
- `wabc.fiba.com` (the English web edition of the WABC manual, all levels): host did not resolve in DNS. Level 2 and 3 English PDFs and the Level 1 French PDF on `assets.fiba.basketball` are verified and are what this document cites. No English Level 1 PDF was found.
- `https://www.basketballengland.co.uk/media/7138/the-pdf-playbook.pdf`: HTTP 403.
- All `breakthroughbasketball.com` pages: HTTP 403.
- `medium.com/the-basketball-dictionary/...` and `medium.com/thebasketballactiondictionary/...`: HTTP 403.
- FIBA Assist Magazine issue PDFs on `d3o24yhvamisba.cloudfront.net` and `ukts.s3.amazonaws.com`: HTTP 404.
- `https://www.espn.com/ncb/2003/0225/1514311.html`: HTTP 202 with no content.
- An official usab.com page for the Youth Development Guidebook: not found.
- `https://assets.fiba.basketball/image/upload/documents-corporate-fiba-official-rules-2024-v10.pdf`: 404; the working name ends `-v10a.pdf`.

Sources that loaded but were only skimmed or summarised by a fetch tool rather than read line by line: `[FIBA-OBRI]`, `[BA-ZB]`, `[USAB]`, `[SG-ICE]`, `[FEARNE-POD]`, `[FIBA-TAG]`, `[NFHS-TD]`, `[BB-RR]`. The Coach's Clipboard, Basketball for Coaches, Hoop Student and Substack pages were read through a page-to-text summariser, so a rule taken from them should be checked against the page (and its diagrams) before it is treated as exact.
