# Realism: the game laid beside real basketball

The owner's point: a simulated season should give data that can be put next to real basketball data, so
realism is measured and not assumed. Until now the engine was calibrated only against its own earlier
numbers (`docs/balance.md`, `basketman lab`). This file is the reference (what real leagues look like, with
sources), the targets the game is held to, and, at the end, the first reading of the engine against them.
The instrument is `basketman realism` (`crates/cli/src/realism.rs`); the targets are in
`docs/realism-targets.json`, which the program embeds.

## The game's rules, and so which leagues to compare with

From `Rules::fiba()` (`crates/core/src/engine/mod.rs:61`), which `World::generate_tiers` uses
(`crates/core/src/league.rs:374`): 4 periods of 10 minutes, overtime 5 minutes, 24 second shot clock,
disqualification at 5 personal fouls, team bonus from the 5th team foul in a period (two free throws),
timeouts 2 in the first half, 3 in the second, 1 per overtime. The three-point line is a 6.75 m arc with
corners at 6.60 m (`court.rs`: `THREE_RADIUS`, `CORNER_X`), as in FIBA. A league has 12 clubs per division,
a double round robin (22 games), three divisions, playoffs in the first only.

So the main reference is **FIBA-rules European club basketball**: the **EuroLeague** (the best data: official
API, three seasons, own calculation), the **EuroCup** (the next tier down, used to scale the lower
divisions), and the strong domestic leagues **ACB** (Spain), **BBL** (Germany), **LNB** (France), **LBA**
(Italy), **BSL** (Turkey) and the Greek league. The **Finnish Korisliiga** was searched for and **not found**
(no page with league statistics could be fetched), so no smaller-league anchor from Finland is used; the
EuroCup and the Greek league stand in for a weaker league. The **NBA** (48 minutes, 7.24 m line, different
defensive rules, 30 teams) is a clearly labelled secondary reference, used only where nothing European was
found (shot distance bins, turnover kinds, assisted shares); NBA-only targets say so in their source and are
soft except where the physics is the same everywhere (finishing at the rim).

Targets are for the **top division**. Lower divisions are less efficient, and the scale is taken from the
EuroLeague to EuroCup step (EuroCup 2024-25 against EuroLeague 2024-25: ORtg 110.3 against 115.5, 3P% 34.0
against 36.2, FT% 75.6 against 77.9, turnovers per 100 possessions 18.7 against 16.9, home win 59% against
61%). That is about -5 ORtg, -2 points of 3P%, -2 of FT% and +2 turnovers per 100 for one step down; the
report's by-division table prints the first three divisions beside each other so the steps can be read.

## How to read the numbers

* "Possessions" everywhere in real data are estimates, `FGA + 0.44 FTA - ORB + TOV` (the EuroLeague API
  publishes no possession count; the ACB's own report uses 0.4 FTA, which lowers possessions by about 0.7 a
  game). `pace`, `ortg`, `tov_per100` and the other per-100 metrics of the instrument use the same formula
  from box scores, so they are compared like with like. The engine's own count of possessions is printed as a
  diagnostic (`diag_engine_poss`): it is about 3% higher than the box-score estimate.
* Real figures marked **own calculation** are means I computed from published team tables or game results
  (arithmetic in the source column). Where several seasons or leagues are given, the target is in their
  middle; the band is wide enough to hold the real spread between seasons and leagues, and a little more.
* **hard** means the game is not credible if it misses (the basic shooting, pace, scoring, foul and
  game-flow numbers, and the play types that decide tactic balance); **soft** means the figure is
  worth steering to but its definition differs between sources or the source is weak.
* **Noise.** Every simulated figure is a ratio of sums over games (or over division-seasons for table and
  player metrics) with a 95% interval from the spread of those clusters. A miss whose interval reaches the
  band is not a finding; the report says "within noise".


## The targets

Target and acceptable band per metric for the top division. **hard** / **soft** as defined above; basis is `sourced` (a published figure, or a mean of published team rows), `derived` (own calculation or scaling, shown in the source) or `none` (not found, no target). The same table is `docs/realism-targets.json`.


### Pace and efficiency

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `pace` | Possessions per team per game, estimated FGA + 0.44 FTA - ORB + TOV | poss/team/game | 73.00 | 70.50 to 75.50 | hard | sourced | EL 72.0 / 72.8 / 74.3 (2023-24/24-25/25-26), EuroCup 75.5 (2024-25): EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `ppg` | Points per team per game | pts | 83.00 | 79.50 to 86.50 | hard | sourced | EL 81.6 / 84.1 / 86.2; ACB 83.2 / 86.2; LNB 80.6 / 83.5; BBL 82.6 (2024-25, incl. playoffs): EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek; BBL Ligareport 2024/25 |
| `ortg` | Points per 100 estimated possessions | pts/100 | 113.0 | 109.0 to 117.0 | hard | sourced | EL 113.3 / 115.5 / 116.0; EuroCup 110.3; BBL 109.1 (own calculation): EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `fga_pg` | Field goal attempts per team per game | FGA | 63.50 | 61.00 to 66.00 | soft | sourced | EL 62.4 / 63.0 / 64.3; BBL 63.9: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `tpa_pg` | Three-point attempts per team per game | 3PA | 26.00 | 24.00 to 28.00 | soft | sourced | EL 25.2 / 26.0 / 26.6; ACB 26.3 / 28.1: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek |
| `fta_pg` | Free-throw attempts per team per game | FTA | 19.30 | 17.00 to 21.50 | soft | sourced | EL 17.3 / 18.8 / 19.8; ACB 20.1 / 20.8; LNB 21.3: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek |
| `ast_pg` | Assists per team per game | AST | 18.00 | 16.00 to 19.50 | soft | sourced | EL 17.6 / 18.5 / 18.9: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `tov_pg` | Turnovers per team per game | TOV | 12.50 | 11.50 to 14.00 | soft | sourced | EL 12.4 / 12.3 / 12.5; EuroCup 14.1: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `stl_pg` | Steals per team per game | STL | 6.50 | 5.80 to 7.50 | soft | sourced | EL 6.6 / 6.4 / 6.4; EuroCup 7.2: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `blk_pg` | Blocks per team per game | BLK | 2.45 | 2.00 to 3.00 | soft | sourced | EL 2.41 / 2.41 / 2.54; EuroCup 2.5: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |

### Shooting

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `fg_pct` | Field goal percentage | % | 47.30 | 45.50 to 49.00 | hard | sourced | EL 47.3 / 47.6 / 47.5; EuroCup 46.3; BBL 44.9: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `two_pct` | Two-point percentage | % | 55.00 | 52.50 to 56.80 | hard | sourced | EL 54.6 / 55.7 / 55.5; EuroCup 54.6; BBL 53.5: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `three_pct` | Three-point percentage (6.75 m line) | % | 35.60 | 33.50 to 37.20 | hard | sourced | EL 36.4 / 36.2 / 36.0; ACB 34.7 / 35.5; LNB 35.4; LBA 35.9; BSL 36.4; BBL 33.6: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek |
| `ft_pct` | Free-throw percentage | % | 76.50 | 73.50 to 79.00 | hard | sourced | EL 77.8 / 77.9 / 78.5; ACB 75.5 / 76.1; EuroCup 75.6; BBL 73.5: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek |
| `three_rate` | Three-point attempt rate, 3PA / FGA | % | 41.50 | 38.50 to 44.50 | hard | sourced | EL 40.4 / 41.3 / 41.3; ACB 41.2 / 43.3; LBA 41.9; BBL 43.4: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek |
| `ft_rate` | Free-throw rate, FTA / FGA | % | 30.00 | 26.00 to 34.00 | hard | sourced | EL 27.7 / 29.9 / 30.8; ACB 31.4 / 32.1; LNB 34.3; EuroCup 34.1; NBA 24.3: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek |
| `efg` | Effective FG%, (FGM + 0.5 3PM) / FGA | % | 54.50 | 52.50 to 56.50 | hard | sourced | EL 54.6 / 55.1 / 54.9; EuroCup 53.1: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `ts` | True shooting %, PTS / (2 (FGA + 0.44 FTA)) | % | 58.50 | 56.50 to 60.50 | soft | sourced | EL 58.3 / 59.0 / 59.0; EuroCup 57.5: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |

### Rebounds, assists, turnovers

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `orb_pct` | Offensive rebound %, ORB / (ORB + opponent DRB) | % | 31.00 | 28.50 to 33.50 | hard | sourced | EL 30.6 / 31.8 / 32.2; EuroCup 31.2; NBA 25.2: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `drb_pct` | Defensive rebound % | % | 69.00 | 66.50 to 71.50 | soft | derived | 100 - orb_pct: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `ast_per_fgm` | Assists per made field goal | ratio | 0.600 | 0.550 to 0.650 | soft | sourced | EL 0.597 / 0.617 / 0.620; EuroCup 0.615: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `tov_per100` | Turnovers per 100 estimated possessions | per 100 | 17.00 | 15.00 to 19.50 | hard | sourced | EL 17.2 / 16.9 / 16.8; EuroCup 18.7 (own calculation, TOV / possessions): EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `tov_pct` | Turnover % of plays, TOV / (FGA + 0.44 FTA + TOV) | % | 14.70 | 13.00 to 16.50 | soft | derived | EL 2024-25: 12.31 / (62.95 + 0.44 x 18.82 + 12.31) = 14.7: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `stl_per100` | Steals per 100 estimated possessions | per 100 | 8.80 | 7.50 to 10.20 | soft | derived | EL steals / possessions: 6.56/72.0, 6.44/72.76, 6.38/74.29 = 9.1 / 8.9 / 8.6: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `blk_per100` | Blocks per 100 estimated possessions | per 100 | 3.40 | 2.70 to 4.20 | soft | derived | EL blocks / possessions = 3.3 / 3.3 / 3.4: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |

### Fouls and officiating

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `pf_pg` | Personal fouls per team per game | PF | 20.40 | 19.00 to 22.00 | hard | sourced | EL 19.8 / 20.4 / 21.0; EuroCup 22.4; ACB report 20.5; NBA 18.6: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `foulout_pg` | Players fouled out per team per game | count | - | - | soft | none | not found: not in the EuroLeague API traditional endpoint, nor BBL report |
| `andone_rate` | And-ones per made field goal | % | 6.00 | 4.50 to 8.00 | soft | sourced | NBA 2024-25 5.4% of made FG (5544 and-ones, own calculation from play-by-play totals); EuroLeague 7.5% by a heuristic over play-by-play events that probably over-counts |
| `foul_shooting_share` | Shooting fouls as a share of all personal fouls | % | 52.60 | 47.00 to 58.00 | soft | sourced | NBA 2024-25: 24066 shooting fouls, 52.6% of all PF (https://www.basketball-reference.com/leagues/NBA_2025_play-by-play.html); no European figure found |
| `foul_offensive_share` | Offensive fouls as a share of all personal fouls | % | 6.80 | 5.50 to 8.50 | soft | sourced | EuroLeague 2024-25: 2.49 offensive fouls a game over 40.8 fouls = 6.1%; NBA 7.5%: EuroLeague 2024-25 official play-by-play of all 306 regular-season games (live.euroleague.net/api/PlayByPlay), own count |
| `pf_home_minus_away` | Home minus away personal fouls per game | fouls | -0.060 | -1.00 to 0.600 | soft | sourced | NBA 2024-25 game-weighted over referee tables: 18.57 home against 18.63 away (own calculation); no European figure found |
| `fta_home_minus_away` | Home minus away free-throw attempts per game | FTA | 0.240 | -0.500 to 1.50 | soft | sourced | NBA 2024-25: 21.79 home against 21.55 away (own calculation, https://www.basketball-reference.com/referees/2025_register.html) |

### Turnovers by kind

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `tov_stolen_share` | Share of turnovers that are steals | % | 52.00 | 47.00 to 57.00 | soft | derived | EL steals / turnovers: 6.56/12.36, 6.44/12.31, 6.38/12.45 = 53.1 / 52.3 / 51.2: EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise) |
| `tov_badpass_share` | Bad passes (incl. out of bounds) as a share of turnovers | % | 46.70 | 40.00 to 54.00 | soft | sourced | NBA 2018-19 to 27 Jan 2019: bad pass 35.58% + bad pass out of bounds 11.08% (own sum of published counts), https://squared2020.com/2019/01/27/the-no-turnover-turnover/ (NBA, partial season; no FIBA figure found) |
| `tov_offfoul_share` | Offensive fouls as a share of turnovers | % | 13.10 | 9.00 to 17.00 | soft | sourced | NBA 2018-19 partial season, 2790/21280: https://squared2020.com/2019/01/27/the-no-turnover-turnover/ (NBA only) |
| `tov_shotclock_share` | Shot-clock violations as a share of turnovers | % | 3.70 | 2.00 to 6.00 | soft | sourced | NBA 2018-19 partial season, 791/21280: https://squared2020.com/2019/01/27/the-no-turnover-turnover/ (NBA only) |

### Shot zones and sources

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `z_mid_share` | Mid-range (two-point shots outside the lane box and the restricted circle) share of FGA | % | 13.00 | 10.00 to 16.00 | soft | sourced | Hudl Instat: mid-range 13% of EuroLeague FGA: https://www.hudl.com/blog/beyond-the-arc-what-actually-wins-in-europe (EuroLeague, chart labels read as 2024-25); its definition ('2-pt shots outside the paint') is close to this one but not identical |
| `z_mid_fg` | Mid-range FG% | % | 39.00 | 36.00 to 43.00 | soft | sourced | Hudl: mid-range eFG 39% (= FG% for two-pointers): https://www.hudl.com/blog/beyond-the-arc-what-actually-wins-in-europe (EuroLeague, chart labels read as 2024-25) |
| `z_c3_share` | Corner threes as a share of FGA | % | 8.00 | 6.50 to 10.00 | soft | sourced | sportando.basketball EuroLeague round 1 stats review: corner attempts 8% of all shots (season not named, low confidence) https://sportando.basketball/en/euroleague-round-1-stats-review-shot-type-values; NBA 9.6% of FGA (own calculation 0.244 x 0.395, NBA 2023-24 https://www.basketball-reference.com/leagues/NBA_2024.html (secondary reference)) |
| `z_c3_of_3pa` | Corner threes as a share of three-point attempts | % | 20.00 | 17.00 to 25.00 | soft | derived | EL 8% of FGA / 41% 3PA rate = 19.5%; NBA 24.4% (2023-24), 24.3% (2024-25) NBA 2023-24 https://www.basketball-reference.com/leagues/NBA_2024.html (secondary reference) |
| `z_c3_fg` | Corner three FG% | % | 38.00 | 35.50 to 41.00 | soft | derived | NBA corner 39.3% (2023-24), 38.8% (2024-25) scaled by EL/NBA 3P% (36.2/36.0 about 1.0): NBA 2023-24 https://www.basketball-reference.com/leagues/NBA_2024.html (secondary reference) |
| `z_atb3_fg` | Above-the-break three FG% | % | 35.00 | 33.00 to 37.00 | soft | derived | NBA non-corner about 35.8% (own calculation (0.366 - 0.244 x 0.393) / 0.756, NBA 2023-24 https://www.basketball-reference.com/leagues/NBA_2024.html (secondary reference)); EL overall 3P% 36.2 with corners 38 gives about 35.5 |
| `ast_share_two` | Assisted share of made two-pointers | % | 54.00 | 48.00 to 60.00 | soft | sourced | NBA 54.0% (2023-24), 54.2% (2024-25), EL assists per FGM equal to NBA's within 0.03: NBA 2023-24 https://www.basketball-reference.com/leagues/NBA_2024.html (secondary reference) |
| `ast_share_three` | Assisted share of made threes | % | 84.00 | 78.00 to 90.00 | soft | sourced | NBA 84.4% (2023-24), 83.4% (2024-25): NBA 2023-24 https://www.basketball-reference.com/leagues/NBA_2024.html (secondary reference) |
| `dist_0_3_share` | Share of FGA within 3 ft (0.91 m) of the basket | % | 23.00 | 18.00 to 28.00 | soft | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 24.1%, 2024-25 22.6% https://www.basketball-reference.com/leagues/NBA_2024.html (NBA only; no European zone table was found) |
| `dist_3_10_share` | Share of FGA 3 to 10 ft | % | 21.00 | 17.00 to 25.00 | soft | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 21.1%, 2024-25 21.4% (NBA only) |
| `dist_10_16_share` | Share of FGA 10 to 16 ft | % | 9.50 | 6.50 to 12.50 | soft | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 9.6%, 2024-25 9.1% (NBA only) |
| `dist_16_3p_share` | Share of FGA from 16 ft out to the three-point line | % | 5.50 | 2.50 to 8.00 | soft | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 5.8%, 2024-25 4.8% (NBA only; the FIBA line is shorter so slightly fewer) |
| `dist_0_3_fg` | FG% within 3 ft | % | 69.70 | 65.00 to 74.00 | hard | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 69.9%, 2024-25 69.6% (NBA only; Hudl and others give no European figure) |
| `dist_3_10_fg` | FG% 3 to 10 ft | % | 45.50 | 41.50 to 49.50 | hard | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 45.2%, 2024-25 46.1% (NBA only) |
| `dist_10_16_fg` | FG% 10 to 16 ft | % | 44.50 | 40.50 to 48.00 | soft | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 44.6%, 2024-25 44.3% (NBA only) |
| `dist_16_3p_fg` | FG% from 16 ft out to the line | % | 40.30 | 36.00 to 44.00 | soft | sourced | Basketball-Reference NBA league average, shooting by distance, 2023-24 40.7%, 2024-25 40.0% (NBA only) |
| `src_catchshoot_share` | Catch-and-shoot share of FGA | % | - | - | soft | none | not found for FIBA leagues (NBA.com tracking pages could not be read) |
| `src_transition_share` | Shots taken in transition, share of FGA | % | - | - | soft | none | not found (see ptype_transition_share for the possession share) |

### Play types

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `ptype_transition_share` | Transition as a share of all possessions | % | 10.00 | 8.00 to 13.00 | hard | sourced | EuroLeague 10% of possessions, stable over a decade (2018-19), Synergy: https://www.eurohoops.net/en/euroleague/810717/euroleague-stats-review-transition-scoring/ (EuroLeague 2018-19, Synergy); Hudl 2024-25 transition eFG 65% |
| `ptype_transition_ppp` | Points per transition possession | ppp | 1.19 | 1.08 to 1.30 | hard | sourced | EuroLeague 1.19 (2018-19, https://www.eurohoops.net/en/euroleague/810717/euroleague-stats-review-transition-scoring/ (EuroLeague 2018-19, Synergy)); clutch-time 1.28 (Foteinakis 2024, jhse.es, last two minutes only) |
| `ptype_pnr_share` | Pick-and-roll possessions (found by a ball screen) as a share of all possessions | % | 38.00 | 30.00 to 44.00 | soft | derived | EuroLeague 42% of half-court possessions (2017-18, 2019-20, Synergy https://www.eurohoops.net/en/euroleague/997552/top-pick-and-roll-defenses-in-euroleague/); x 0.90 non-transition = 38%. Synergy counts plays that began as a pick-and-roll, the engine counts a logged Screen event |
| `ptype_pnr_ppp` | Points per pick-and-roll possession | ppp | 0.970 | 0.880 to 1.06 | soft | sourced | EuroLeague 0.97 (2019-20 through round 17) https://www.eurohoops.net/en/euroleague/997552/top-pick-and-roll-defenses-in-euroleague/; 1.00 in 2017-18 |
| `ptype_spotup_ppp` | Points per spot-up possession | ppp | 1.03 | 0.930 to 1.13 | soft | sourced | EuroLeague 1.03 (2019-20 through round 8) https://www.eurohoops.net/en/euroleague/968999/the-spot-up-stars-of-euroleague/amp/ |
| `ptype_halfcourt_ppp` | Points per half-court possession | ppp | 1.02 | 0.940 to 1.10 | soft | derived | EuroLeague 0.96 (2019-20 through round 17), raised toward the level of today's efficiency (the 2017-18 league 0.99 per possession against the 2024-25 ORtg of 115): https://www.eurohoops.net/en/euroleague/810717/euroleague-stats-review-transition-scoring/ (EuroLeague 2018-19, Synergy) |
| `ptype_post_share` | Post-up possessions as a share of all possessions | % | - | - | soft | none | not found (EuroLeague 2022-24 clutch post-up PPP 0.78 only, not a full-game share) |
| `ptype_iso_share` | Isolation share of possessions | % | - | - | soft | none | not found for full games (clutch 23.6% is not representative; an 8% figure is from a search snippet that could not be fetched) |
| `ptype_cut_share` | Cut share of possessions | % | - | - | soft | none | not found |
| `second_chance_pts_pg` | Second-chance points per team per game | pts | 14.10 | 11.50 to 16.50 | soft | derived | NBA 2023-24 13.79 second-chance points from 10.54 ORB (own calculation, https://www.foxsports.com/articles/nba/2023-24-nba-offensive-rating-team-offense-rankings-and-efficiency-stats) = 1.31 points per ORB, times EuroLeague ORB 10.8 per game (2024-25) |
| `fastbreak_pts_pg` | Fast-break (transition) points per team per game | pts | 8.70 | 6.90 to 10.50 | soft | derived | 10% of possessions x 1.19 ppp x 72.8 possessions = 8.7 (own calculation); EuroLeague team range 6.9 to 13.4 in 2018-19 https://www.eurohoops.net/en/euroleague/810717/euroleague-stats-review-transition-scoring/ (EuroLeague 2018-19, Synergy) |
| `pts_off_tov_pg` | Points off turnovers per team per game | pts | - | - | soft | none | not found (only team ranges for NBA fast-break points) |
| `zone_poss_share` | Share of possessions played against a zone defense | % | - | - | soft | none | not found for any FIBA league; NBA 3.6% early 2025-26 (Second Spectrum via Stein Line, https://marcstein.substack.com/p/talking-zone-defense-nba-big-men) is not a European comparison |
| `press_poss_share` | Share of possessions against a press | % | - | - | soft | none | not found (NBA figures only from an unfetched page) |

### Game level

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `home_win_pct` | Home win percentage | % | 62.00 | 57.00 to 67.00 | hard | sourced | EL 65.4 / 61.1 / 63.7; EuroCup 59.4; BBL 64 / 58; NBA 54.4: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games) |
| `margin_mean_abs` | Mean absolute final margin | points | 10.80 | 9.50 to 12.50 | hard | sourced | EL 10.5 / 10.5 / 10.2; EuroCup 12.1; NBA 12.75: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games) |
| `margin_median_abs` | Median absolute final margin | points | 9.00 | 8.00 to 10.50 | soft | sourced | EL 9 / 9 / 9; EuroCup 9.5; NBA 10: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games) |
| `close5_share` | Share of games decided by 5 points or fewer | % | 29.00 | 25.00 to 33.00 | hard | sourced | EL 32.7 / 29.7 / 29.7; EuroCup 28.3; BBL 27 / 23: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games) |
| `blowout20_share` | Share of games decided by 20 points or more | % | 13.00 | 9.00 to 18.00 | hard | sourced | EL 11.4 / 13.4 / 10.5; EuroCup 20.6; BBL 15 / 24 (read from a chart); NBA 21.1: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games) |
| `ot_rate` | Share of games that go to overtime | % | 4.00 | 2.50 to 6.00 | soft | sourced | EL 4.9 / 3.3 / 3.9; EuroCup 6.1; BBL 4.4; NBA 4.9: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games) |
| `halftime_leader_win_pct` | Share of games won by the half-time leader | % | 72.00 | 68.00 to 77.00 | soft | sourced | EL 69.6 / 74.7 / 71.5; EuroCup 75.4: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games) |
| `comeback10_rate` | Share of games the winner trailed by 10 or more at some time | % | - | - | soft | none | not found (only 'third-quarter leader loses' 20% in EuroLeague) |
| `comeback15_rate` | Share of games the winner trailed by 15 or more at some time | % | - | - | soft | none | not found |

### Competitive balance

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `winpct_sd` | Standard deviation of team win % across a division (22 games each) | fraction | 0.176 | 0.150 to 0.200 | soft | derived | sqrt(talent sd 0.14^2 + binomial luck 0.25/22) = 0.176; the raw sd depends on the number of games, so the talent and Noll-Scully rows below carry the comparison |
| `winpct_talent_sd` | Spread of true team strength, sqrt(sd^2 - 0.25/games), in win % | fraction | 0.140 | 0.100 to 0.170 | hard | derived | EL 0.133 / 0.117 / 0.096, ACB 0.152 / 0.157, LNB 0.158, EuroCup 0.152, NBA 0.150; own calculation from population sd of win % (EL 0.1587 / 0.1448 / 0.1259, ACB 0.1746 / 0.1792, LNB 0.1826, EuroCup 0.1925, NBA 0.1601): EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek; NBA 2024-25 https://www.basketball-reference.com/leagues/NBA_2025.html (secondary reference) |
| `noll_scully` | Noll-Scully ratio for a 22-game season: sd of win % over the sd of pure luck (0.1066) | ratio | 1.65 | 1.40 to 1.90 | hard | derived | Real ratios depend on the number of games (EL 1.85 / 1.69 / 1.55 over 34-38 games, ACB 2.09 / 2.04, LNB 2.00, EuroCup 1.63 over 18; own calculation of win % sd over sqrt(0.25/games): EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games)), so the target is the ratio a talent sd of 0.14 gives over 22 games: sqrt(0.14^2 + 0.1066^2) / 0.1066 = 1.65, band from talent sd 0.10 to 0.17 (1.37 to 1.88) |
| `ortg_sd` | Standard deviation across teams of offensive rating | pts/100 | 5.00 | 3.80 to 6.80 | soft | sourced | EL 4.2 / 4.5 / 4.0, ACB 4.5 / 5.4, LNB 4.2 / 6.4, LBA 5.4, BSL 4.2, Greek 7.3, EuroCup 5.9 (population sd of team ORtg, own calculation): EuroLeague 2023-24/2024-25/2025-26, own calculation from official API team per-game tables (https://api-live.euroleague.net/v3/competitions/E/statistics/teams/traditional?SeasonMode=Single&SeasonCode=E2024&phaseTypeCode=RS&statisticMode=perGame; E2023 and E2025 likewise); Basketball-Reference international team tables, 2024-25 (basketball-reference.com/international/): ACB, LNB, LBA, BSL, Greek |
| `drtg_sd` | Standard deviation across teams of defensive rating | pts/100 | - | - | soft | none | not found: needs opponent possessions, only offensive rating spread was computed |
| `prev_win_r` | Correlation of a team's win % with its win % the season before | r | 0.610 | 0.450 to 0.750 | soft | sourced | EL 2023-24 to 2024-25 r = 0.637 (n = 17 common teams, own calculation); NBA 0.608 / 0.618 / 0.578 (2021-22 to 2024-25): EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API game results (api-live.euroleague.net/v3/competitions/E/seasons/E2024/games); NBA 2024-25 https://www.basketball-reference.com/leagues/NBA_2025.html (secondary reference) |
| `payroll_win_r` | Correlation of payroll with win % | r | 0.390 | 0.200 to 0.550 | soft | derived | NBA relative salary about 0.38-0.40 (2011-2022) from a search summary that could not be fetched (unverified); the ZenGM page calls it 'fairly noisy and weak'; no European figure found |
| `strength_win_r` | Correlation of squad strength at the season's start with win % | r | - | - | soft | none | not found (no real analogue of the game's overall rating) |

### Players

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `rotation_size` | Players per team at 10 minutes a game or more (and in half the games) | players | 10.50 | 9.50 to 12.00 | hard | sourced | EL 10.9 / 10.5 / 10.5 (range 8-14 over teams); NBA 10.0: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `starters_mpg` | Minutes per game of the five most-used players, mean over teams | min | 24.30 | 22.50 to 26.00 | hard | sourced | EL 24.5 / 24.3 / 23.6; NBA about 31.7 (upper bound, trade splits): EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `bench_mpg` | Minutes per game of the 6th to 10th most-used players | min | 16.00 | 14.00 to 18.00 | hard | sourced | EL 16.1 / 16.0 / 15.9: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `top_mpg` | Highest individual minutes per game in a team, mean over teams | min | - | - | soft | none | the league maximum (EL 31.7 / 31.3 / 31.0, own calculation) is not the same quantity as the mean of team maxima computed here; the team figure was not found |
| `top_scorer_ppg` | Scoring leader's points per game (qualifying: 70% of games) | ppg | 20.00 | 18.50 to 22.00 | hard | sourced | EL 19.4 / 21.1 / 19.4 (official API, regular season; Wikipedia gives 19.5 / 20.4 / 19.0, unresolved): EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025). Real leagues have 18-20 clubs, this one 12, which lowers the expected leader slightly |
| `top10_ppg_mean` | Mean points per game of the ten best scorers | ppg | 17.50 | 15.50 to 19.50 | soft | sourced | EL top-10 means 16.7 / 18.1 / 17.6 (own calculation from the published leaders list): EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `top_scorer_pts_share` | A team's top scorer's share of its points, mean over teams | % | 19.00 | 16.00 to 23.00 | hard | sourced | EL 18.1 / 19.7 / 18.8; NBA 17.4: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `top_fga_share` | A team's top shooter's share of its field goal attempts | % | - | - | soft | none | not found |
| `three_pct_p10` | 10th percentile of individual 3P% among qualifiers | % | 29.40 | 27.00 to 32.00 | soft | sourced | EL 29.0 / 29.2 / 29.9 (qualifier rule of the API, not the 1.5 attempts a game used here): EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `three_pct_p90` | 90th percentile of individual 3P% among qualifiers | % | 43.50 | 40.50 to 46.50 | soft | sourced | EL 44.7 / 43.1 / 42.6: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `ft_pct_p10` | 10th percentile of individual FT% among qualifiers | % | 68.00 | 64.00 to 72.00 | soft | sourced | EL 70.4 / 66.4 / 67.1: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `ft_pct_p90` | 90th percentile of individual FT% among qualifiers | % | 90.00 | 87.00 to 93.00 | soft | sourced | EL 89.8 / 90.9 / 89.3: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `reb_leader_rpg` | League rebound leader, per game | rpg | 7.50 | 6.50 to 9.50 | soft | sourced | EL 7.3 / 8.3 (18 games) / 7.1: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `ast_leader_apg` | League assist leader, per game | apg | 7.40 | 6.50 to 8.50 | soft | sourced | EL 7.2 / 7.5 / 7.5: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `height_guard_cm` | Average height of rotation guards (PG+SG, SF+PF, C) | cm | 191.3 | 188.0 to 195.0 | soft | sourced | EuroLeague 2024-25 rosters (api-live.euroleague.net/v2/competitions/E/seasons/E2024/clubs/{CLUB}/people), own mean over players with statistics; NBA G 192.8 / F 202.3 / C 210.1 cm (NBA 2024-25 Basketball-Reference tables, own calculation (https://www.basketball-reference.com/leagues/NBA_2025.html)) |
| `weight_guard_kg` | Average weight of rotation guards | kg | 88.60 | 84.00 to 93.00 | soft | sourced | EuroLeague 2024-25 rosters (same source); NBA G 90.1 / F 100.6 / C 111.7 kg |
| `height_forward_cm` | Average height of rotation forwards (PG+SG, SF+PF, C) | cm | 202.6 | 199.5 to 205.5 | soft | sourced | EuroLeague 2024-25 rosters (api-live.euroleague.net/v2/competitions/E/seasons/E2024/clubs/{CLUB}/people), own mean over players with statistics; NBA G 192.8 / F 202.3 / C 210.1 cm (NBA 2024-25 Basketball-Reference tables, own calculation (https://www.basketball-reference.com/leagues/NBA_2025.html)) |
| `weight_forward_kg` | Average weight of rotation forwards | kg | 100.0 | 96.00 to 104.0 | soft | sourced | EuroLeague 2024-25 rosters (same source); NBA G 90.1 / F 100.6 / C 111.7 kg |
| `height_center_cm` | Average height of rotation centers (PG+SG, SF+PF, C) | cm | 209.2 | 206.0 to 212.5 | soft | sourced | EuroLeague 2024-25 rosters (api-live.euroleague.net/v2/competitions/E/seasons/E2024/clubs/{CLUB}/people), own mean over players with statistics; NBA G 192.8 / F 202.3 / C 210.1 cm (NBA 2024-25 Basketball-Reference tables, own calculation (https://www.basketball-reference.com/leagues/NBA_2025.html)) |
| `weight_center_kg` | Average weight of rotation centers | kg | 110.2 | 105.0 to 115.0 | soft | sourced | EuroLeague 2024-25 rosters (same source); NBA G 90.1 / F 100.6 / C 111.7 kg |
| `age_mean` | Average age of rotation players | years | 28.70 | 27.00 to 30.00 | soft | sourced | EL 28.8 / 28.7 / 28.8 (28.9 minutes-weighted); NBA 25.8: EuroLeague 2023-24/2024-25/2025-26, own calculation from the official API player/team tables (api-live.euroleague.net, SeasonCode E2023/E2024/E2025) |
| `peak_age` | Age at which players are best (a design check on the hidden rating, not box-score output) | years | 27.00 | 25.50 to 28.50 | soft | derived | 26.3 (Vaci et al. 2019, from a search summary, PDF not read) and about 27 (Frontiers 2025, search summary): both unverified, https://eprints.whiterose.ac.uk/159117/1/ |

### Coaching

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `timeouts_pg` | Timeouts called per team per game | timeouts | 2.06 | 1.70 to 2.40 | soft | sourced | EuroLeague 2024-25: 1260 team timeouts in 306 games = 4.12 per game, 2.06 per team (TV timeouts, 2.87 a game, are extra and not modelled): EuroLeague 2024-25 official play-by-play of all 306 regular-season games (live.euroleague.net/api/PlayByPlay), own count |
| `subs_pg` | Players coming on per team per game (substitution events) | subs | 21.40 | 18.00 to 25.00 | soft | sourced | EuroLeague 2024-25: 13107 'IN' events / 306 / 2 = 21.4 (15.1 distinct stoppages per team): EuroLeague 2024-25 official play-by-play of all 306 regular-season games (live.euroleague.net/api/PlayByPlay), own count |
| `adjust_pg` | Changes of system by the bench per team per game | changes | - | - | soft | none | not found (no league tracks it) |
| `challenge_use_pg` | Coach's challenges per team per game | challenges | 0.580 | 0.400 to 0.800 | soft | sourced | EuroLeague 2024-25: 355 challenge events in 306 games = 1.16 a game, 0.58 per team: EuroLeague 2024-25 official play-by-play of all 306 regular-season games (live.euroleague.net/api/PlayByPlay), own count; the NBA analogue is about 1.5 a game (1842 challenges in 1230 games, a blog total, unverified) |
| `challenge_success` | Share of coach's challenges that succeed | % | - | - | soft | none | not found for FIBA or EuroLeague (the play-by-play has no result field); NBA 58-63% in 2023-24 and 2024-25 only from search summaries that could not be fetched |

### Officiating

| id | what | unit | target | band | kind | basis | source and arithmetic |
|---|---|---|---|---|---|---|---|
| `crew_foul_sd` | Standard deviation between referees of fouls per game (both teams) | fouls | 1.28 | 0.800 to 1.80 | soft | sourced | NBA 2024-25, 60 referees with at least 40 games, range 34.6 to 39.9 fouls (own calculation from https://www.basketball-reference.com/referees/2025_register.html); no European figure found |
| `call_accuracy` | Share of reviewed calls that were correct (last two minutes) | % | 96.00 | 94.00 to 98.00 | soft | sourced | NBA Last Two Minute reports: 97% (2025), 96% (2024), 96% (2023), 95% (2022) per a third-party tracker https://atlhawksfanatic.github.io/L2M; no European figure found |

### Count

114 metrics in the file: 80 with a target from a published figure (`sourced`), 17 with a target derived from published figures (`derived`), 17 **not found** (no target; the instrument still measures them where it can). The not-found ones: `foulout_pg`, `src_catchshoot_share`, `src_transition_share`, `ptype_post_share`, `ptype_iso_share`, `ptype_cut_share`, `pts_off_tov_pg`, `zone_poss_share`, `press_poss_share`, `comeback10_rate`, `comeback15_rate`, `drtg_sd`, `strength_win_r`, `top_mpg`, `top_fga_share`, `adjust_pg`, `challenge_success`.

## The instrument: `basketman realism`

```sh
cargo run --release -p basketman-cli -- realism [--games N] [--seasons N] [--out DIR] [--external FILE] [--targets FILE]
```

* `--games N` (default 3000): league games of the top division, every club with its own tactics and its
  head coach on the bench (`lineup()` as in `lab league`), each **recorded**; and N/4 games in each of the
  two lower divisions for the by-division table. Pairs are all ordered pairs of 12 clubs in generated
  leagues (seeds 7000 and up); each game has its own seed. About 20 seconds for 3000 games on 6 threads.
* `--seasons N` (default 12): full seasons of the three-division career world (`new_career_tiers`, played
  with `Command::Advance` and `PlayRound`, three seasons to a world so that last season's table is there),
  read when the regular season ends: tables, competitive balance, minutes, scoring leaders, bodies, ages.
  About 5 seconds a season on 6 threads (about 65 s for 12).
* `--out DIR` writes `DIR/realism.json`: every metric with value, interval, target, band, verdict, the
  unmeasurable list and the metrics without target.
* `--external FILE`: games played elsewhere, to be laid beside the baseline. The file is a JSON array, or
  `{"label": "...", "games": [...]}`, of items that are either a `Fixture` as the world saves it
  (`{"home","away","division","result":{...}}`) or a bare `GameResult` (`{"score","periods","boxes","possessions"}`),
  or a saved world (an object with `fixtures`; playoff games are skipped). Everything that can be found from
  box scores is computed from it (pace, efficiency, shooting, fouls, game flow, tables and balance, minutes
  and scoring spread, if the fixtures carry team and division ids) and printed in an extra column;
  recording-based metrics (zones, play types, turnover kinds) need recordings and show only for the baseline.
  A bench-run season of an agent club is thus laid beside the AI baseline and the real targets.
* `--targets FILE` replaces the embedded `docs/realism-targets.json`.
* `RAYON_NUM_THREADS` sets the worker threads of the league games; results are gathered in game order and are
  identical for any number of threads (a test checks 1 against 3).

How a metric is computed: every metric is a ratio of sums over clusters (a game; or one division in one
season for table and player metrics), and its 95% interval is the delta-method interval from the spread of the
clusters (Student t below 30 clusters). Medians use an order-statistic interval. Percentages are reported as
percentages. The home advantage in the league games is the engine's own (`HOME`, `engine/floor.rs:61`, and
`floor.rs:2373`); the crowd and travel of a managed calendar are not part of those games.

How the event log is read (`event_rows`): possessions are runs of events by the same side (shots, rebounds,
turnovers, free throws, fast breaks, screens, post-ups); their count is within 3% of the engine's own count and
the points in them add up to the final score (both checked by a test). A possession is classed
transition (a `FastBreak` event, or a first shot with source Transition), post (a `PostUp` event), pick and roll
(a `Screen` event, or a roll finish), then by its first shot's source: spot-up (catch and shoot), isolation
(drive or pull-up), cut. These are the engine's rough classes, not Synergy's: where they differ from the real
definitions the target says so.

### What cannot be computed yet

| metric | what the engine or the recording would need |
|---|---|
| `challenge_use_pg`, `challenge_success` | no coach's challenge exists; a challenge command, a referee review of a call, and a log of which calls were wrong |
| `crew_foul_sd` | no referee in the recording; a crew id per game (and a whistle per foul) |
| `call_accuracy` | no notion of a wrong call; each whistle (and each missed one) must be logged with the contact it judged and a correct flag, as the NBA Last Two Minute reports do |
| `ptype_handoff_share`, `ptype_offscreen_share` | hand-offs and screens away from the ball are not events; only the ball handler's `Screen` is logged |
| `ptype_pnr_split` | ball-handler and roll-man possessions cannot be told apart except by the Roll shot source |
| `shot_open_share` | contest is a 0 to 1 number per shot, not the distance of the closest defender at release used for "open" and "wide open" |
| `career_length` | needs 15 or more seasons of a world (retirement exists, `retire.rs`); not part of the default run |
| `drtg_sd` and others without target | computed but no real figure found |

## Doubtful sources, plainly

* **EuroLeague and EuroCup team numbers** are my own means of the official API's team tables, not published
  league figures; possessions are an estimate (the API has none). Basketball-Reference's EuroLeague 2025-26
  points per game is about 2 below the API for five clubs; the API is internally consistent and was used.
* **EuroLeague play types** (transition 10% and 1.19 PPP; pick-and-roll 42% of half-court possessions, 0.97 PPP;
  spot-up 1.03 PPP) are from Eurohoops articles quoting Synergy and are **2017-18 to 2019-20**, older than the
  seasons of the team table. League efficiency has risen since (ORtg 113 to 116 now); the half-court PPP target was
  raised for that, by judgement. The Euroleague's own pages returned HTTP 429 to every fetch, so a current
  play-type table was not found (a search snippet with 39% of possessions in pick-and-roll was not fetched and is not used).
* **Shot zones**: no European zone table could be read. The Hudl article gives mid-range share (13%) and eFG
  (39%) in charts whose season label (2024-25) is inferred from chart text; the corner share (8%) is from an
  article whose season is not named. Everything else in the zone and distance rows is **NBA** Basketball-Reference.
* **Home win, margins, overtime, half-time leaders** for the EuroLeague are my own counts from the official game
  results (quarter scores); the BBL figures are from the league's report, with playoff games mixed into the
  box totals, and its 2023-24 margin buckets were read from a pie chart.
* **Player rows** are the EuroLeague API's own, my calculation (the Wikipedia infobox gives slightly different
  scoring leaders, unresolved). Qualifiers for 3P% and FT% follow the API's rule, the game's follow "70% of games and
  1.5 attempts a game", so the percentile targets are loose. The EuroLeague and-one and shooting-foul counts are a
  heuristic over play-by-play and probably over-count; the NBA figure is used for the target.
* **Unverified** (search summaries only, pages not read): the NBA coach's-challenge success rates (58% to 63%), the
  payroll-wins correlation (0.38 to 0.40), the peak-age figures (26.3 and about 27). They back `payroll_win_r` and
  `peak_age` only, both soft. The Cleaning the Glass pages show the in-progress 2026-27 season and were not used for
  any target.
* **Not found**: the Finnish Korisliiga (nothing fetchable), ACB and BBL player distributions, European budget to
  wins correlation, EuroLeague challenge success rate, fouled-out players per game, points off turnovers, league
  fast-break points, zone and press shares for any FIBA league, NBA timeouts and substitutions per game, career length.
* **Lower divisions** have no sourced target of their own: the step is scaled from EuroLeague to EuroCup (above).


### The table

Sorted worst first (distance beyond the band in half-band widths; a half-band is half the band's width). `noise` means the miss is inside the 95% interval's reach of the band: not a finding.

| metric | simulated | 95% interval | target | band | kind | verdict |
|---|---|---|---|---|---|---|
| `top_scorer_ppg` | 47.01 | 39.73 to 54.30 | 20.00 | 18.50 to 22.00 | hard | **outside by 14.3** (high) |
| `top10_ppg_mean` | 28.44 | 25.45 to 31.43 | 17.50 | 15.50 to 19.50 | soft | **outside by 4.5** (high) |
| `ptype_transition_ppp` | 1.76 | 1.75 to 1.77 | 1.19 | 1.08 to 1.30 | hard | **outside by 4.1** (high) |
| `ortg_sd` | 11.94 | 10.63 to 13.24 | 5.00 | 3.80 to 6.80 | soft | **outside by 3.4** (high) |
| `blowout20_share` | 30.97 | 29.31 to 32.62 | 13.00 | 9.00 to 18.00 | hard | **outside by 2.9** (high) |
| `top_scorer_pts_share` | 33.06 | 30.82 to 35.30 | 19.00 | 16.00 to 23.00 | hard | **outside by 2.9** (high) |
| `foul_shooting_share` | 31.64 | 31.31 to 31.97 | 52.60 | 47.00 to 58.00 | soft | **outside by 2.8** (low) |
| `dist_0_3_fg` | 85.95 | 85.64 to 86.25 | 69.70 | 65.00 to 74.00 | hard | **outside by 2.7** (high) |
| `dist_3_10_fg` | 31.86 | 31.48 to 32.25 | 45.50 | 41.50 to 49.50 | hard | **outside by 2.4** (low) |
| `ppg` | 71.69 | 71.36 to 72.02 | 83.00 | 79.50 to 86.50 | hard | **outside by 2.2** (low) |
| `ortg` | 100.4 | 99.97 to 100.8 | 113.0 | 109.0 to 117.0 | hard | **outside by 2.2** (low) |
| `starters_mpg` | 29.67 | 29.44 to 29.90 | 24.30 | 22.50 to 26.00 | hard | **outside by 2.1** (high) |
| `margin_mean_abs` | 15.54 | 15.11 to 15.96 | 10.80 | 9.50 to 12.50 | hard | **outside by 2.0** (high) |
| `margin_median_abs` | 13.00 | 12.00 to 13.00 | 9.00 | 8.00 to 10.50 | soft | **outside by 2.0** (high) |
| `ast_share_two` | 37.47 | 36.96 to 37.97 | 54.00 | 48.00 to 60.00 | soft | **outside by 1.8** (low) |
| `winpct_sd` | 0.242 | 0.231 to 0.254 | 0.176 | 0.150 to 0.200 | soft | **outside by 1.7** (high) |
| `fta_pg` | 13.31 | 13.12 to 13.49 | 19.30 | 17.00 to 21.50 | soft | **outside by 1.6** (low) |
| `noll_scully` | 2.27 | 2.16 to 2.38 | 1.65 | 1.40 to 1.90 | hard | **outside by 1.5** (high) |
| `peak_age` | 23.42 | 22.84 to 23.99 | 27.00 | 25.50 to 28.50 | soft | **outside by 1.4** (low) |
| `winpct_talent_sd` | 0.217 | 0.204 to 0.230 | 0.140 | 0.100 to 0.170 | hard | **outside by 1.4** (high) |
| `pf_pg` | 17.02 | 16.90 to 17.15 | 20.40 | 19.00 to 22.00 | hard | **outside by 1.3** (low) |
| `ts` | 53.96 | 53.76 to 54.16 | 58.50 | 56.50 to 60.50 | soft | **outside by 1.3** (low) |
| `dist_3_10_share` | 30.07 | 29.68 to 30.45 | 21.00 | 17.00 to 25.00 | soft | **outside by 1.3** (high) |
| `z_c3_of_3pa` | 12.34 | 12.00 to 12.69 | 20.00 | 17.00 to 25.00 | soft | **outside by 1.2** (low) |
| `ft_rate` | 21.97 | 21.65 to 22.29 | 30.00 | 26.00 to 34.00 | hard | **outside by 1.0** (low) |
| `fg_pct` | 43.75 | 43.57 to 43.94 | 47.30 | 45.50 to 49.00 | hard | **outside by 1.0** (low) |
| `z_mid_share` | 7.06 | 6.95 to 7.17 | 13.00 | 10.00 to 16.00 | soft | **outside by 1.0** (low) |
| `two_pct` | 50.40 | 50.13 to 50.66 | 55.00 | 52.50 to 56.80 | hard | **outside by 1.0** (low) |
| `tov_shotclock_share` | 0.050 | 0.035 to 0.066 | 3.70 | 2.00 to 6.00 | soft | **outside by 1.0** (low) |
| `ptype_spotup_ppp` | 1.22 | 1.21 to 1.23 | 1.03 | 0.930 to 1.13 | soft | **outside by 0.9** (high) |
| `ast_pg` | 14.44 | 14.32 to 14.57 | 18.00 | 16.00 to 19.50 | soft | **outside by 0.9** (low) |
| `efg` | 50.75 | 50.54 to 50.96 | 54.50 | 52.50 to 56.50 | hard | **outside by 0.9** (low) |
| `z_c3_share` | 5.05 | 4.90 to 5.21 | 8.00 | 6.50 to 10.00 | soft | **outside by 0.8** (low) |
| `home_win_pct` | 52.87 | 51.08 to 54.65 | 62.00 | 57.00 to 67.00 | hard | **outside by 0.8** (low) |
| `second_chance_pts_pg` | 9.50 | 9.38 to 9.62 | 14.10 | 11.50 to 16.50 | soft | **outside by 0.8** (low) |
| `close5_share` | 21.87 | 20.39 to 23.35 | 29.00 | 25.00 to 33.00 | hard | **outside by 0.8** (low) |
| `dist_16_3p_share` | 0.690 | 0.658 to 0.721 | 5.50 | 2.50 to 8.00 | soft | **outside by 0.7** (low) |
| `dist_16_3p_fg` | 33.59 | 31.72 to 35.45 | 40.30 | 36.00 to 44.00 | soft | **outside by 0.6** (low) |
| `weight_guard_kg` | 81.38 | 81.19 to 81.56 | 88.60 | 84.00 to 93.00 | soft | **outside by 0.6** (low) |
| `age_mean` | 26.18 | 25.98 to 26.38 | 28.70 | 27.00 to 30.00 | soft | **outside by 0.5** (low) |
| `tov_pct` | 17.27 | 17.14 to 17.41 | 14.70 | 13.00 to 16.50 | soft | **outside by 0.4** (high) |
| `fta_home_minus_away` | -0.942 | -1.32 to -0.562 | 0.240 | -0.500 to 1.50 | soft | **outside by 0.4** (low) |
| `halftime_leader_win_pct` | 78.49 | 77.00 to 79.98 | 72.00 | 68.00 to 77.00 | soft | **outside by 0.3** (high) |
| `z_c3_fg` | 34.68 | 34.00 to 35.35 | 38.00 | 35.50 to 41.00 | soft | **outside by 0.3** (low) |
| `dist_10_16_fg` | 39.59 | 39.06 to 40.13 | 44.50 | 40.50 to 48.00 | soft | **outside by 0.2** (low) |
| `ptype_transition_share` | 7.45 | 7.36 to 7.54 | 10.00 | 8.00 to 13.00 | hard | **outside by 0.2** (low) |
| `fga_pg` | 60.57 | 60.39 to 60.75 | 63.50 | 61.00 to 66.00 | soft | **outside by 0.2** (low) |
| `ast_share_three` | 90.74 | 90.38 to 91.10 | 84.00 | 78.00 to 90.00 | soft | **outside by 0.1** (high) |
| `tov_offfoul_share` | 8.51 | 8.26 to 8.76 | 13.10 | 9.00 to 17.00 | soft | **outside by 0.1** (low) |
| `ast_per_fgm` | 0.545 | 0.542 to 0.548 | 0.600 | 0.550 to 0.650 | soft | **outside by 0.1** (low) |
| `payroll_win_r` | -0.107 | -0.445 to 0.231 | 0.390 | 0.200 to 0.550 | soft | outside by 1.8, within noise |
| `reb_leader_rpg` | 10.09 | 9.27 to 10.91 | 7.50 | 6.50 to 9.50 | soft | outside by 0.4, within noise |
| `ast_leader_apg` | 6.11 | 5.34 to 6.87 | 7.40 | 6.50 to 8.50 | soft | outside by 0.4, within noise |
| `pf_home_minus_away` | 0.790 | 0.524 to 1.06 | -0.060 | -1.00 to 0.600 | soft | outside by 0.2, within noise |
| `ot_rate` | 2.17 | 1.65 to 2.69 | 4.00 | 2.50 to 6.00 | soft | outside by 0.2, within noise |
| `ft_pct_p90` | 86.46 | 85.21 to 87.71 | 90.00 | 87.00 to 93.00 | soft | outside by 0.2, within noise |
| `prev_win_r` | 0.752 | 0.655 to 0.848 | 0.610 | 0.450 to 0.750 | soft | outside by 0.0, within noise |
| `bench_mpg` | 14.14 | 13.74 to 14.54 | 16.00 | 14.00 to 18.00 | hard | inside |
| `ft_pct` | 76.74 | 76.43 to 77.04 | 76.50 | 73.50 to 79.00 | hard | inside |
| `orb_pct` | 29.19 | 28.95 to 29.43 | 31.00 | 28.50 to 33.50 | hard | inside |
| `pace` | 71.43 | 71.28 to 71.59 | 73.00 | 70.50 to 75.50 | hard | inside |
| `rotation_size` | 9.93 | 9.77 to 10.09 | 10.50 | 9.50 to 12.00 | hard | inside |
| `three_pct` | 34.17 | 33.93 to 34.41 | 35.60 | 33.50 to 37.20 | hard | inside |
| `three_rate` | 40.93 | 40.60 to 41.26 | 41.50 | 38.50 to 44.50 | hard | inside |
| `tov_per100` | 19.42 | 19.27 to 19.56 | 17.00 | 15.00 to 19.50 | hard | inside |
| `andone_rate` | 7.05 | 6.91 to 7.18 | 6.00 | 4.50 to 8.00 | soft | inside |
| `blk_per100` | 3.84 | 3.70 to 3.98 | 3.40 | 2.70 to 4.20 | soft | inside |
| `blk_pg` | 2.74 | 2.64 to 2.84 | 2.45 | 2.00 to 3.00 | soft | inside |
| `dist_0_3_share` | 18.87 | 18.71 to 19.04 | 23.00 | 18.00 to 28.00 | soft | inside |
| `dist_10_16_share` | 9.44 | 9.30 to 9.58 | 9.50 | 6.50 to 12.50 | soft | inside |
| `drb_pct` | 70.81 | 70.57 to 71.05 | 69.00 | 66.50 to 71.50 | soft | inside |
| `fastbreak_pts_pg` | 8.93 | 8.81 to 9.05 | 8.70 | 6.90 to 10.50 | soft | inside |
| `foul_offensive_share` | 6.86 | 6.65 to 7.07 | 6.80 | 5.50 to 8.50 | soft | inside |
| `ft_pct_p10` | 69.56 | 68.48 to 70.65 | 68.00 | 64.00 to 72.00 | soft | inside |
| `height_center_cm` | 211.4 | 211.0 to 211.8 | 209.2 | 206.0 to 212.5 | soft | inside |
| `height_forward_cm` | 204.1 | 203.8 to 204.3 | 202.6 | 199.5 to 205.5 | soft | inside |
| `height_guard_cm` | 192.1 | 191.9 to 192.3 | 191.3 | 188.0 to 195.0 | soft | inside |
| `ptype_halfcourt_ppp` | 0.994 | 0.989 to 0.998 | 1.02 | 0.940 to 1.10 | soft | inside |
| `ptype_pnr_ppp` | 0.931 | 0.923 to 0.938 | 0.970 | 0.880 to 1.06 | soft | inside |
| `ptype_pnr_share` | 30.17 | 29.39 to 30.95 | 38.00 | 30.00 to 44.00 | soft | inside |
| `stl_per100` | 9.93 | 9.84 to 10.02 | 8.80 | 7.50 to 10.20 | soft | inside |
| `stl_pg` | 7.10 | 7.03 to 7.16 | 6.50 | 5.80 to 7.50 | soft | inside |
| `subs_pg` | 21.90 | 21.86 to 21.94 | 21.40 | 18.00 to 25.00 | soft | inside |
| `three_pct_p10` | 27.16 | 26.34 to 27.98 | 29.40 | 27.00 to 32.00 | soft | inside |
| `three_pct_p90` | 41.68 | 40.78 to 42.59 | 43.50 | 40.50 to 46.50 | soft | inside |
| `timeouts_pg` | 2.08 | 2.06 to 2.10 | 2.06 | 1.70 to 2.40 | soft | inside |
| `tov_badpass_share` | 50.30 | 49.82 to 50.78 | 46.70 | 40.00 to 54.00 | soft | inside |
| `tov_pg` | 13.87 | 13.76 to 13.98 | 12.50 | 11.50 to 14.00 | soft | inside |
| `tov_stolen_share` | 51.15 | 50.72 to 51.59 | 52.00 | 47.00 to 57.00 | soft | inside |
| `tpa_pg` | 24.79 | 24.61 to 24.97 | 26.00 | 24.00 to 28.00 | soft | inside |
| `weight_center_kg` | 109.8 | 109.1 to 110.5 | 110.2 | 105.0 to 115.0 | soft | inside |
| `weight_forward_kg` | 98.70 | 98.41 to 98.98 | 100.0 | 96.00 to 104.0 | soft | inside |
| `z_atb3_fg` | 34.10 | 33.84 to 34.36 | 35.00 | 33.00 to 37.00 | soft | inside |
| `z_mid_fg` | 39.37 | 38.75 to 40.00 | 39.00 | 36.00 to 43.00 | soft | inside |

Measured but without a real target (not found or not comparable): `adjust_pg` 1.48, `comeback10_rate` 15.67, `comeback15_rate` 3.97, `drtg_sd` 6.89, `foulout_pg` 1.04, `press_poss_share` 4.47, `pts_off_tov_pg` 16.36, `ptype_cut_share` 3.23, `ptype_iso_share` 12.65, `ptype_post_share` 16.95, `src_catchshoot_share` 37.47, `src_transition_share` 6.64, `strength_win_r` 0.528, `top_fga_share` 31.54, `top_mpg` 31.36, `zone_poss_share` 32.10.

## First reading (2026-11-16, engine at commit 495f8d2)

Taken on commit **495f8d2** ("Close lab::league after the merge", the base of branch `realism`) with the engine
untouched. **Three other agents are changing the engine as this is written** (fast breaks after steals, referees
and wrong calls, body orientation and footwork); this reading is of the engine before any of that, and the
numbers below should be re-taken after each merges (`basketman realism --out DIR`, compare `realism.json`).
Run: 3000 recorded top-division league games (seeds 7000..), 750 each in divisions 2 and 3, 12 seasons of the
three-division world (4 worlds, seeds 9000..); about 3.5 minutes on a shared machine at 6 threads (the run
was repeated and gave the same numbers to every digit). Intervals are 95%. Season-level and player-level
metrics rest on 12 division-seasons only and are noisy; league-game metrics rest on 3000 games and are not.
Misses smaller than half a band width are listed in the table but are not worth a sentence.

### Where the game is least like real basketball, ranked

The ranking is by distance beyond the band, hard targets first among equals. The explanations are readings of
the code, not experiments; each says what would have to change. None of this was changed.

1. **One star takes the game over (hard).** The leading scorer averages **47.0 ppg** (interval 39.7 to 54.3;
   target 20.0, band 18.5 to 22.0, the EuroLeague's 19.4 to 21.1); the top ten average 28.4 (real 17.5); a team's
   top scorer takes **33%** of its points (real 19%, band 16 to 23) and 31.5% of its shots. `basketman 5`
   prints the cause: a centre with 39 shots and 0.2 threes a game, 50 ppg in 30 minutes. A man's appetite is the
   role's usage (1.6 for the primary option, `tactics.rs:292-299`) times his scoring skill
   (`engine/mod.rs:266`); the Isolation lead is simply the highest usage (`engine/floor.rs:1250`), the post man
   the best of `(PostMoves + Strength) * (0.7 + 0.3 usage)` (`floor.rs:1252`), and usage also lowers the
   quality of look a man needs (`floor.rs:2413-2420`). Nothing pushes back: no defense doubles a hot post man,
   no fatigue or focus cost grows with touches. Change: usage must have decreasing returns inside a game
   (a defender who has been burned sends help; the engine already has `Help` events), and the post set
   should rotate its target; target a top share of shots of 25% to 28%.
2. **Offense is 12 points a hundred too low and the cause is two-point shooting (hard).** ORtg **100.4**
   (100.0 to 100.8; target 113, band 109 to 117), points per team 71.7 (83), FG% 43.8 (47.3), 2P% 50.4 (55.0),
   eFG% 50.8 (54.5). Pace is inside the band (71.4, target 73), 3P% (34.2), 3PA rate (40.9%), FT% (76.7) and
   offensive rebounding (29.2%) are inside, so the shortfall is inside the arc and at the line. Measured by
   distance from the basket: within 3 ft the engine makes **86.0%** (real 69.7%, band 65 to 74) while from 3 to 10 ft
   it makes **31.9%** (real 45.5%, band 41.5 to 49.5), and takes 30.1% of its shots from 3 to 10 ft (real 21%).
   Finishing at the rim is too safe and everything just outside it is too hard. In the recorded games the shots
   in that ring are post and drive finishes taken at contest 0.6 to 1.0 (the opening minutes of a probe game, league seed 7, game seed 5:
   post jumpers from `Mid` at contest 0.59 to 0.82, one in four made). Reading: the launch model's error grows
   with distance and the contest (`floor.rs:3596`, `shot.rs:167`), so a layup alone under the rim is near
   certain while any shot with a hand up from 1.5 to 3 m is poor; there is no middle case (a floater, a hook, a
   shot over a smaller man) and no penalty for a rim shot against a big. Change: look at what a contested
   finish at 0.5 to 2 m is worth against real figures (45% at 3 to 10 ft), likely by letting the shot error depend on the hand's
   height and reach against the shooter's release instead of a flat contest number (the plan in
   `docs/engine-physics.md`), then re-measure `dist_*`.
3. **Fast breaks are layups (hard).** Points per transition possession **1.76** (1.75 to 1.77; real 1.19, band
   1.08 to 1.30; clutch 1.28), shots with the Transition source have an eFG of 94.8%. The share (7.5% against
   10%) and the fast-break points a game (8.9 against about 8.7) are inside, so the engine has about the right
   number of breaks and finishes them far too well. A `FastBreak` event is only raised when the ball is live,
   the defense has at most two men back and the offense has more men ahead (`floor.rs:2227-2236`): numbers up,
   by construction, then an unguarded finish. Real transition includes the early offense where the defense is
   mostly back. Change: let the defense's recovery count (a trailing man should contest or foul), count early
   offense in the same class when comparing; the fast-break-after-steals work in progress touches exactly this.
4. **Games are decided too often and by too much (hard).** 31.0% of games are won by 20 or more (real 13%,
   band 9 to 18), mean margin **15.5** (10.8), median 13 (9), only 21.9% by 5 or fewer (29%); the half-time
   leader wins 78.5% (72%). Across a division the spread of true strength is **0.217** in win % (real 0.14,
   band 0.10 to 0.17; Noll-Scully 2.27 against 1.65 for 22 games), and the spread of team ORtg 11.9 against 5.0. The
   clubs are generated with a squad quality `9.1 - 1.05 x division + uniform(0, 1.8)` (`league.rs:321`), and
   `docs/balance.md` found a gap of 1 attribute point wins 73% of games and 1.5 wins 86%. Real top divisions
   are about two thirds as spread. Change: narrow the quality spread of the generated clubs (a width of about 1.1
   instead of 1.8 should bring talent sd near 0.14, to be measured) or flatten the map from rating to result;
   this is a generator and rating-scale decision, not an engine bug. The home win is 52.9% (51.1 to 54.7; real 62%,
   band 57 to 67), also too low, from the engine's small home terms (`floor.rs:61`, `floor.rs:2373`: a shot error
   of 0.016 and 0.8% of make rate); the real home edge in referee calls (home free throws +0.24 a game in the
   NBA, here **-0.94**, interval -1.3 to -0.6, and home fouls +0.8) is not modelled at all and belongs to the
   referee work.
5. **Too few free throws, too few fouls (hard).** FTA/FGA **22.0%** (real 30%, band 26 to 34), 13.3 free throws
   a team a game (19.3), 17.0 personal fouls (20.4, band 19 to 22). Shooting fouls are 31.6% of fouls (the NBA 52.6%,
   soft). A shooting foul is a dice roll per shot: 6% to 24% at the rim by contest, 1.5% to 7.5% on mid-range shots, 0.5% to
   2.5% on threes (`floor.rs:2379-2385`); the other fouls are rare events on the ball (reach-in, charge)
   and the rest. Change: make contact on the shooter produce the foul (the engine already tracks bodies and contact in
   `engine/contact.rs`), calibrate against 30% FTA/FGA and about half of fouls being shooting fouls; overlaps the referee agent.
   One more oddity: 1.04 players per team per game foul out with only 17 fouls a team (no real figure found; it means
   the fouls fall on the same few men).
6. **Starters play 5 minutes too long (hard).** The five most-used men play **29.7** minutes (real 24.3, band
   22.5 to 26.0; the bench 6th to 10th 14.1, inside), rotation 9.9 deep (real 10.5, inside, low edge). The default
   plan gives starters 30 and the rest 10 (`tactics.rs:519`); balance finding 8 says more minutes cost less than a worse
   bench. Substitution counts agree with reality (21.9 against 21.4 a team) and timeouts (2.08 against 2.06). Change: the
   fatigue curve (`engine/mod.rs` `need`) and the default plan, so that 24 to 26 is the best plan for balanced squads.
7. **Defense and ball handling: more turnovers, fewer assists.** Turnovers 19.4 per 100 possessions (17.0,
   band 15 to 19.5, upper edge; 13.9 a game against 12.5), steals 9.9 per 100 (8.8, inside), assists 14.4 a game
   (18.0) and 0.545 per made basket (0.60); only 37.5% of made twos are assisted (NBA 54%) while 90.7% of threes are
   (84%). Shot-clock violations are 0.05% of turnovers (NBA 3.7%): a late possession ends in a forced shot
   (`Source::Forced`, 4.9% of shots) and not in a violation. Change: more passing between the post man and the
   others (it follows from item 1); keep an eye on the turnover rate once fast breaks change.
8. **Where the threes and the mid-range come from (soft).** Corner threes are 12.3% of three-point attempts
   (real about 20%, NBA 24%), 5.1% of all shots (8%); mid-range 7.1% of shots (Hudl 13%); spot-up possessions
   score 1.22 per possession (1.03). The drawn spots of the offensive sets put the shooters above the break
   (`tactics.rs` boards); a corner spot is a short pass from the wing and the sets use it rarely. Change: the stock boards.
9. **Players do not look like players (soft).** Guards weigh 81 kg (EuroLeague 88.6; heights are inside), the
   average age of the men who play is 26.2 (28.7), and the best age in the league is 23.4 against 27 (a check on
   the hidden rating, not box-score output; the real figure is from unverified summaries): the generator
   (`league.rs` `generate_player`) makes young, light squads. Second-chance points are 9.5 a game (14.1) with the
   offensive rebound rate inside, so putbacks are worth too little.
10. **The league has one tactical colour (cannot be compared, listed because it is big).** 32.1% of possessions
    are played against a zone (no FIBA figure was found; the NBA is 2.5% to 3.6%, not a European number) and
    the bench changes its system 1.48 times a team a game. The generated heads of the bench have a 20% weight of
    zone preferences (`staff.rs:144-150`) and the in-game adjustment drifts toward zones (`floor.rs:1054-1065`).
    Unknown how much of it is real; to be answered with a source for European zone usage first.

**Inside the band** (no finding): pace, 3P%, FT%, 3PA/FGA, offensive rebound %, bench minutes, rotation size,
substitutions, timeouts, blocks, steals per 100, height of every position, weight of forwards and centres,
and-ones, offensive-foul share, share of turnovers that are steals or bad passes, the fast-break points,
half-court PPP, pick-and-roll PPP, the individual 3P% and FT% spread at the bottom, previous-season win
correlation (0.75 against 0.61, within noise). **Within noise** (12 seasons are too few to say): payroll to
wins (-0.11, interval -0.45 to 0.23, against about 0.39 in the NBA, unverified), the rebound and assist leaders,
overtime rate (2.2%, 4% real), and the home foul margin.

**Lower divisions.** League games in divisions 1, 2 and 3: ORtg 100.4 / 95.8 / 93.0, FG% 43.8 / 42.4 / 41.6,
3P% 34.2 / 33.5 / 32.2, FT% 76.7 / 74.2 / 71.8, pace 71.4 / 70.1 / 69.5, fouls 17.0 / 15.3 / 13.9. The steps
(ORtg -4.6 then -2.8, 3P% -0.7 then -1.3, FT% -2.5 then -2.5) are about as large as the EuroLeague to EuroCup
step (-5, -2, -2), except that fouls fall by two a game per step where the real EuroCup commits more fouls
(22.4 against 20.4 in the EuroLeague), a sign that the lower-division foul and free-throw numbers are going the wrong way.



## Finishing at the rim (the `stance` branch)

The first reading above found open layups nearly automatic and everything just outside them too hard. A finish's
launch error used to be an angle, so it vanished with the distance flown (a ball released half a metre from the rim
cannot be thrown wide). The model in `engine/floor/finish.rs` and `engine/shot.rs`:

* `Errors::at_ring`: a layup, floater or dunk has its error in metres **at the ring**, whatever the distance.
* The hand he finishes with (`finish_hand`: the dominant hand from his own side, the far hand when his man is on
  the near side and he has the touch). A hand that is not his own, across the body, or toward the man who is
  contesting costs error; finishing and footwork take part of every cost off.
* How he arrives (`finish_arrival`): the speed he came in at (`Stance::peak`, his fastest of the last second), the
  angle across the board, finishing from behind the rim; one multiplier, replacing the old break-only sprint term.
  The stance multiplier and the officials' disturbed shot (`FOULED_ERR`) are the other two factors, each applied
  once, in `shot_errors`.
* Dunks (`dunk_trouble`): a bump, a man who has lost his legs (balance) and a jump too far from the rim turn a dunk
  into a miss or a foul.
* The floater (`Style::Floater`, `takes_floater`): taken by a driver past a protected rim in place of the layup,
  with its own, high-arc error model, a lower release and less sensitivity to the arm in front of it. Post players
  turn into a short hook from 1.1 m, not a layup.
* Help (`rim_rotation`): on a drive, on a gather and on a break the best-placed other defender who can get to the
  rim in time steps into the lane; the contest, the block hands and `verticality` do the rest.
* Shooting fouls were 20% to 40% too rare: `foul_chance` is raised by the same factor on every kind of shot,
  and a defender moving into the shooter counts for less (`VERT_MOVING`).

`basketman realism --games 1500 --seasons 0`, before and after (same worlds, same seeds):

| metric | before | after | real target (band) |
|---|---|---|---|
| FG% within 3 ft | 81.7 | 69.6 | 69.7 (65 to 74) |
| FG% 3 to 10 ft | 27.1 | 41.9 | 45.5 (41.5 to 49.5) |
| share of FGA 0 to 3 ft / 3 to 10 ft | 17.6 / 26.7 | 14.4 / 29.3 | 23 / 21 |
| FT rate (FTA/FGA) | 22.6 | 30.0 | 30 (26 to 34) |
| FG% / 2P% | 42.1 / 47.4 | 43.0 / 49.1 | 47.3 / 55.0 |
| points per 100 possessions | 97.9 | 101.4 | 113 (109 to 117) |
| transition, points per possession with a shot | 1.56 | 1.49 | 1.19 (1.08 to 1.30) |
| three-point % | 35.3 | 35.0 | 35.6 |

Not reached: the league is still 4 points of FG% and 11 of offensive rating under the real one (the rest is in
items 1, 3 and 8 above, not at the rim); fewer shots are taken within 3 ft (14%, real 23%) because drivers now float it
from further out; a transition possession with a shot is worth 1.49 (the instrument counts only possessions with a
shot or a fast-break event, so 1.19, Synergy's figure for every transition possession, is not the same quantity: per
chance, including turnovers and misses without a shot, `lab transition` gives 1.10); layups from 1.3 to 2.2 m that were
not floated are still made only 20% of the time (a swarmed rim).
