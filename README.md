# Desktop Bug Companion

A Windows desktop overlay app that spawns small cursor-reactive creatures on top of your real desktop. The first included creature is a procedural spider with planted-foot IK, alternating gait groups, stop-look-go stalking, cursor chase, and realistic retreat when the cursor darts toward it.

The overlay is not a game canvas. It uses a frameless, always-on-top, per-pixel transparent Qt window and Windows hit-testing so empty space stays click-through while spider pixels can be grabbed. The only intentional visible pixels are the spider and its soft shadow/highlight pixels. Left-drag a spider to pick it up; releasing it preserves throw inertia and makes it scrabble away startled.

## Reference target

The included spider is tuned to match a small dark desktop spider similar to the reference video: it crawls over existing apps/documents near the real cursor, pauses to look, approaches/stalks, chases briefly, and retreats from sudden cursor movement.

## Moods, expressions, and play

Each spider now carries a small continuous mood made of four values: valence (happy to sad), arousal (calm to excited), affection, and curiosity. Personalities set the resting mood, situations push it around, and it eases back over time. The mood drives how the spider looks and what it chooses to do. None of this changes the planted-foot gait; expression rides on top of the existing walk.

Because the overlay is a top-down view, the spider cannot express feeling by standing up or looking at you. It expresses feeling through the shape and motion of its antennae, its eyes, and how it holds and wiggles its body.

### Expressive antennae

The two front feelers are no longer fixed decorations. Their shape is rebuilt every frame from the current mood:

- A happy or affectionate spider curls its antennae into raised, recurved hooks.
- A sad or sleepy spider lets them sag and droop.
- When ranging a target before a pounce, both feelers stiffen, straighten, and converge forward like a rangefinder.
- A cuddling spider curls them softly inward as if to wrap.
- An inspecting spider extends and probes with them.

Antennae appear on every model, including older ones, because they are generated from sensible defaults. A model can tune or disable them through an optional `appearance.antennae` block.

### Eyes

Eyes now open, narrow, and gaze. Arousal widens them, sleepiness and sadness lower the lids, and a genuinely content spider squints into happy upturned arcs. The pupils track the spider's current point of interest, and affection adds a soft blush.

### Body language without moving the legs

The spider can wiggle, nod, crouch, rear, and waggle its abdomen while its planted feet stay exactly where they are. This is deliberate. Real spiders shift the body over still legs, so the new motion reads as alive without turning into a crab walk. Body wiggle, abdomen wag, crouch depth, and squash are all separate from the gait solver.

### Jumping and the pounce

When a bold or hunting spider locks onto a target it performs a full pounce sequence:

1. It crouches low into a jump-prep coil.
2. It often wiggles or nods while ranging the distance.
3. It springs to the target. The body lifts and scales up while a shrunken shadow stays on the ground to sell the hop, since there is no real vertical axis in a top-down view.
4. On landing it rolls one of several outcomes: a happy cuddle, a startled run-away, or a catch where it reaches out with antennae and front legs.

There is also a lighter spring action for excited little hops that are not full pounces.

### Playing with other spiders

When more than one spider is on screen and social play is enabled, spiders seek each other out. One may approach and invite another, then the two chase, circle, and tumble together before breaking off. A spider can also fall into pure zoomies when its arousal runs high.

### Curious inspection

A curious spider will walk up to its point of interest, lower its body, and inspect it with extended antennae and small probing motions before either escalating or losing interest and wandering off.

### Choosing a mood at runtime

The mood baseline is set by personality, but you can override it for every spider from the tray **Mood** menu (Auto, Playful, Cuddly, Curious, Calm). **Auto** restores each spider's personality-defined baseline. Two personalities ship specifically for this: **Playful** (energetic, social, quick to play) and **Cuddly** (gentle, affectionate, slow and close). **Hunter** now has a special mouse-hunting mode: it notices the cursor from farther away, bursts closer in short stop-start runs, freezes to observe when the cursor is still, and whips around after catching it. The hunter now moves noticeably faster on its approach and chase, and when it freezes to watch a still cursor it holds completely still rather than shuffling its feet. **Jumper** is a new small-hop personality that bounces constantly while roaming, approaching, or chasing. To see either, add a few slots in the settings window with the Playful or Social temperament and turn social play on; a screen full of Jumper spiders is the quickest way to trigger the pounce.

### Movement styles

The preset now has three movement choices: **Classic**, **Lively**, and **Skitter**. Classic keeps the original gait. Lively lifts the feet visibly, replants them while turning, and uses front legs/pedipalps to probe nearby objects. **Skitter** is the new third option: it is based on Lively, but adds faster leg flicks, shorter step cooldowns, sharper body acceleration, and tiny burst-burst-stop pauses so the spider runs in quick successions like the reference gif. The default preset is set to Skitter so it is easy to test immediately.

### Stillness when stopped

When a spider comes to a stop it now leaves its feet exactly where they landed instead of tidying them back into a neutral rest pose or twitching a toe now and then. A stopped spider is genuinely still. This reads most clearly on the hunter, which can freeze mid-stalk and stay frozen, but it applies to every spider whenever it stops moving. The legs still settle naturally while the spider is slowing down, and a genuinely broken pose (a leg folded across the body, say) is still quietly corrected; what is gone is the constant idle fidgeting.

### Rolling and tumbling

A happy, excited spider will sometimes tuck its legs in and roll, spinning through a turn or two as it tumbles a short way across the desk before popping back up and carrying on. It is a pure flourish with no target, most common in playful, high-energy spiders, so setting a spider to the **Playful** mood (or using the Playful personality) is the easiest way to see it. A spider close to the cursor may also do a quick tumble away from the pointer when it is in a good mood.

### Weaving webs

Spiders can spin silk webs, and they build them the way a real spider does, one thread at a time. Web-spinning is the **Webber** personality's specialty rather than something every spider does, so by default the webs you see are made by Webbers. You can still grant the **Weave web** ability to any spider through its skill list if you want a non-Webber to build too. A web does not appear all at once. The spider walks its silk into place strand by strand, and the pale threads grow under it as it goes until the finished shape is complete.

The build order follows a real orb-weaver. The spider first runs a bridge line and a frame anchored into the corner, then lays the spokes (radii) one at a time, returning toward the hub between each. It reinforces the hub, then spins a widely spaced temporary spiral outward from the hub as a working guide. Finally it spins the sticky capture spiral inward from the rim, and the temporary guide spiral fades away as the capture spiral takes its place. Because the spinnerets lead the body slightly, the silk tip runs a little ahead of the spider as it traces each thread, so it reads as the spider actively drawing the line rather than the line appearing on its own.

There are a few different finished forms. The signature one is a sector orb tucked into a screen corner, with its outermost spokes lying along the two walls so the web hugs the corner. The spider also builds full circular orbs out in the open, funnel-weaver sheets with a tubular retreat folded into the corner, and loose cobweb tangles with vertical gumfoot drop-lines. A Webber favours corners but often strings a web in an open part of the screen too, so its webs end up scattered around rather than all hugging the edges. It does not blanket the desktop: a Webber spins a handful of webs, at most seven on screen at once, and each finished web that is still intact reduces its urge to build another, so it settles down once a few are up. A torn web does not count toward that, so damage leaves the Webber wanting to put things right.

Other spiders treat finished webs as part of the furniture. A spider will sometimes walk onto a completed web and pluck it a few times to test its bounce, and the whole web wobbles and settles in response, the ripple fading out from wherever it was plucked. This is on by default for every spider through the **Walk on webs** ability.

You can break a web yourself by dragging the mouse pointer across it. Moving the pointer through the silk snaps the strands it passes over, so a web tears in the places you swipe rather than disappearing all at once: a quick pass opens a ragged hole, and several passes can shred most of it. A web only tears while the pointer is actually moving over it, so simply resting the cursor on a web does nothing. A Webber dislikes a broken net. When it notices torn silk it travels to the damaged web and re-knits the missing strands, working across the damage until the web is whole again.

A web in progress is not owned forever. If the spider building it gets distracted, flees the cursor, or is picked up and dragged, it leaves the unfinished web behind, and any spider that comes across an abandoned, half-built web may adopt it and finish it off, even though it did not start it. Webbers are especially keen to do this and will cross the screen to complete someone else's work. A web that was barely begun is simply dropped rather than left as a stray stub.

Two abilities cover all of this. **Weave web** -- building webs, repairing torn ones, and finishing abandoned ones -- is the Webber's specialty and is on for Webbers by default. **Walk on webs** -- walking onto a finished web and plucking it -- is a common ability that every spider has. You can add or remove either one per spider from the right-click menu or in the settings window like any other ability. The quickest way to watch webs go up, get walked on, get torn, and get repaired is a slot on the `moss_velvet_orbweaver` model carrying the **Weave web** ability, alongside a few ordinary spiders.

### Shooting webs at the cursor

Spiders can also fire sticky silk straight at your mouse pointer. This is a different kind of silk from the decorative webs above: instead of building a structure in a corner, the spider flings a glob of web at the cursor itself.

There are two shots, each its own ability:

- **Shoot trapping web** (`shoot_web`) pins its target roughly where the glob lands. Fired at the pointer it holds the cursor in place, and you break free by **moving the mouse around a bit**. Wiggling fills a small struggle meter; holding still lets it drain, so a quick shake tears the silk in well under a second while a motionless mouse stays caught until a built-in safety timer releases it. The same shot is what a spider flings at a fly to web it from range.
- **Web-shove to wall** (`wall_web`) is the flashier finisher. The glob slams the pointer to the nearest wall along the spider's firing line and pins it against the edge, where the same wiggle-to-escape rule peels it off.

Before either shot the spider crouches, faces its target, and converges its feelers into a rangefinder, then lets the glob fly with a small recoil. The silk is thrown, not guided: the spider aims once, leading a moving target by where it is actually going, and the glob then flies straight, so a quick sideways move can make it miss. Correcting in flight is the **Silk tracking** talent, unlocked at level 9 in the skill tree, rather than something every spider starts with. The silk reads as it travels: a trailing thread with a sticky head, then a radiating splat over the trapped target that stretches and tears as the captive struggles against it. The skills are named for the action rather than the target because the spider uses the very same silk on a fly as it does on your pointer.

The dedicated **Trapper** personality hunts the pointer with this silk. It stalks the cursor in the patient stop-start way the Hunter does, then webs it instead of pouncing, favouring an in-place trap and occasionally shoving the pointer to a wall. A still cursor is treated as an easy mark. Any other spider can be given the skills too; without the Trapper temperament they fire only rarely, so an ordinary spider catching your pointer is an occasional surprise rather than a constant one.

Because these shots are the only behaviour that moves your real pointer, there is a single master switch. The tray **Interaction** menu has **Let spiders web-trap the mouse**, on by default; turning it off frees the pointer immediately, cancels any glob in flight, and stops spiders ever moving your cursor again. The two skills are also toggleable per spider from the right-click menu like any other ability, and a hard maximum hold time means a trap can never lock your pointer indefinitely even if you never move it. On non-Windows platforms the silk still animates but never moves the pointer, matching the rest of the overlay's Windows-only cursor behaviour. To watch the pointer get caught, give a couple of jumping-spider slots the **Shoot trapping web** and **Web-shove to wall** abilities in the settings window.

## Flies

Flies are small autonomous prey that buzz around the screen for the spiders to hunt. They are not spiders and have no personalities or skills of their own; they exist to give the spiders something to chase, trap, and eat.

A fly enters the screen and flits around with an insect-like buzz: quick darting turns, the odd hover, and flickering wings. It steers away from any spider that comes too close, breaking into a faster panic dash when one is nearly on top of it, and it stays inside the screen by turning back at the edges.

Every spider reacts to flies using whatever its personality is best at, and it now keeps after one that stops moving instead of losing interest. A **Hunter** stalks a fly and pounces, driving straight in even when the fly holds still. A **Trapper** flings a glob of sticky silk that wraps the fly in a clinging web splat where it was hit, with no thread trailing back, then closes the distance and eats it. A **Jumper** bounces after it and leaps on it. Calmer spiders such as the cuddly or curious ones still give chase and pounce, just less relentlessly. Whatever the approach, when a spider reaches a fly it crouches over the catch and works it with its front legs, the way a real spider turns prey in its grasp, while the rest of its body stays planted, then it leaves a little scatter of disassembled remains that fades over the next few seconds and moves on a little happier.

Flies can also blunder into a finished web and get stuck. A trapped fly struggles and tugs the silk, so the web visibly quivers, and that movement acts as a distress signal: nearby spiders notice the wider commotion and rush over to claim the easy meal. A fly that nobody eats will eventually wrench itself loose and fly off again.

By default the flies crawl out of a small **nest** object you can drag anywhere on the screen, and the flies themselves are draggable too: grab one with the mouse and it dangles there buzzing, ignored by the spiders, until you let go and it zips off along the toss. You can even hunt with a spider in your hand: drag a web-shooting spider near a fly and it locks onto the fly and webs it from where you hold it. The fly spawner has its own controls:

- The settings window has a **Flies** group with a master on/off switch, a **spawn every (min) to (max)** range in seconds (set both to the same value for a fixed timer, or leave a gap for a random interval), a **max flies at once** cap, and a checkbox for whether flies emerge from the movable nest or simply drift in from the screen edges.
- The tray **Flies** menu has the same on/off switch, three quick spawn-rate presets (**Sparse**, **Normal**, **Swarm**), **Release a fly now** to drop a single fly on demand, a toggle for the nest, and options to add another nest or reset the nest to its default spot.

Flies are saved in the preset's `settings` under a `flies` block (`enabled`, `min_interval`, `max_interval`, `max_flies`, `spawner`), so a launched overlay picks up changes live the same way it does every other setting. Turning flies off frees the flock immediately and the spiders go back to reacting only to the cursor and to each other.

## Naming spiders and hover labels

Every spider can carry a name. Right-click a spider and choose **Name this spider** (or **Rename**), type a name, and confirm. The name is stored on that spider and follows it as it walks.

Hover the mouse over a named spider and a small dark label appears just above it showing the name. The label is drawn upright in screen space so it stays readable whatever direction the spider is facing, and it disappears when the cursor moves away. Hover detection is sampled from the real cursor position every frame, so it works even when the pointer is not generating window events.

If you want every name on screen at once, the right-click menu and the tray **Interaction** menu both offer **Always show spider names**. Turn it on and each named spider keeps its label visible without needing a hover.

Right-click naming stays available even when **Let spiders react to the cursor** is turned off, so you can label and inspect spiders without them fleeing the pointer. The one tradeoff is that a spider only intercepts the mouse when either cursor reaction or naming is enabled. If you turn both off, spiders become fully click-through and can no longer be named or dragged until you re-enable one of them.

## Cages

A cage is a draggable, resizable pen you can drop on the desktop to keep chosen spiders in one place. Open the tray **Interaction → Cages** menu and choose **Add a cage** to drop one in the middle of the screen, or right-click and use **Add a cage here** to place it where you clicked.

Any spider whose body is inside the cage when it appears is enclosed, and any spider you later drag into the cage is enclosed as well. An enclosed spider roams, plays, and reacts normally, but its own wandering will never carry it through the wall. The inside of the cage stays click-through, so it behaves like a fence rather than a solid panel and does not block anything underneath it.

You stay in control of the cage:

- **Move it** by dragging anywhere on its border. Every enclosed spider is carried along with it.
- **Resize it** by dragging any of the four corner grips. If you shrink it, the enclosed spiders are nudged inward so they stay inside the smaller pen.
- **Take a spider out** by dragging it through the wall and dropping it outside. Releasing it outside removes it from that cage, and it roams free again.

You can have several cages at once, each holding its own spiders, and a spider only belongs to one cage at a time. To clear them, use **Cages → Remove all cages**; the spiders inside are simply released and keep wandering. Moving or resizing a cage repaints cleanly and leaves no faint outline behind at the old position.

## Desktop window and folder awareness

On Windows, the overlay now samples visible top-level app windows a few times per second. Since the companion overlay must remain always-on-top to draw on the desktop, real windows cannot physically cover its pixels; instead, spiders now simulate depth by fading out as their body crosses into a real window rectangle, then fading back in as they crawl out near an edge. Hidden spiders also stop capturing mouse clicks, so the window underneath stays usable.

File Explorer folder windows act like soft portals when two or more folder windows are open. If a spider crawls deep enough into one Explorer folder window, it can vanish there, reappear just inside another folder window, and crawl out from that folder edge. This uses live Explorer windows, not desktop shortcut icons.

## Fluffy Friends pack

Six soft, high-fluff procedural spiders ship alongside the originals. They render entirely from the `appearance` block (no PNG assets), so they are light and fully editable.

- **Snowtuft** — cream-white plush, big gentle eyes, soft blush. Defaults to Cuddly.
- **Honey Tuft** — warm amber curly-hair look with six eyes. Defaults to Mellow.
- **Periwinkle** — pastel blue-lavender with large eyes and blush. Defaults to Bashful.
- **Rosella** — rose-pink, the cutest of the set with the biggest eyes and full blush. Defaults to Clingy.
- **Mossback** — earthy sage green with six eyes. Defaults to Curious.
- **Cocoa Plush** — rich chocolate plush, the largest of the set with eight eyes. Defaults to Bold.

Eight new personalities expand the behavior range, each with its own resting emotional baseline:

- **Bold** — fearless and confident; approaches readily, rarely startles, holds its ground.
- **Grumpy** — slow and territorial; keeps to itself and is easily annoyed by close, fast movement.
- **Zoomy** — hyper and restless; very fast, wanders constantly, rarely sits still.
- **Mellow** — calm and content; unbothered and relaxed without being sleepy.
- **Clingy** — wants to be near the cursor; high affection, follows persistently, stays close.
- **Bashful** — extra timid; large personal space and flees far, but moves at a normal pace.
- **Nope** — panics when the mouse approaches, then rapidly backward-jumps in a zigzag chain before running away.
- **Drifter** — builds momentum, breaks traction, leans into wide sideways slides, sometimes drifts circles/corners on its own, and keeps sliding when grabbed or thrown.

Add one slot per model in the settings window to try the whole pack at once.

## Plush Tarantulas (hybrid 2D)

A second pack uses the **sprite_rig** renderer (the same "hybrid 2D" system as the existing hybrids) for cuter, more realistic fluff: each spider is built from generated PNG body parts (abdomen, cephalothorax, three leg segments, shadow) layered by the rig. They feature dense fur fringes, soft downy halos, brushed surface fur, chunky bodies, thick legs, big eyes, and blush.

- **Plush Rose** — soft dusty-rose plush with a faint heart marking and big blushing eyes. Defaults to Clingy.
- **Curly Cutie** — golden honey-brown curly-hair tarantula, very fluffy. Defaults to Mellow.
- **Bluebell Plush** — teal-blue legs with a warm peach abdomen, greenbottle-blue inspired. Defaults to Curious.
- **Cocoa Fluff** — rich chocolate plush, the largest with the thickest legs and eight eyes. Defaults to Bold.
- **Snowpuff** — cream-white and downy with big eyes and blush. Defaults to Cuddly.
- **Berry Knee** — near-black body with warm berry-orange foot "socks," red-knee inspired. Defaults to Grumpy.

Because they are sprite_rig models, their look lives in `models/<id>/assets/*.png` and is fully editable or replaceable; the `colors` block still drives the procedural bits (leg joints, feet, pedipalps, antennae, eyes, and blush), so keep it in step with the art. Thickness, eye size, blush, and body proportions are tuned per model through the `appearance` block (`leg_segment_thickness`, `leg_tip_thickness`, `foot_bulb`, `eye_scale`, `eye_count`, `cute_blush`, `abdomen_scale`, `cephalothorax_scale`).

## Requirements

- Windows 10/11 recommended for the real click-through desktop overlay.
- Python 3.10+ for development mode.
- `PyQt5` and `pyinstaller` from `requirements.txt`.

Install dependencies:

```bat
python -m pip install -r requirements.txt
```

## Run in development mode

Double-click:

```bat
run_dev.bat
```

or run manually:

```bat
set PYTHONPATH=%CD%\src
python -m desktop_bug.config_ui
```

The settings UI opens first. Choose model, personality, count, save/load a preset, then click **Save and launch overlay**. The main settings window includes the same launch-time options shown before launch:

- The **Model** dropdown shows a small live thumbnail beside each model name, so you can see the spider shape/color before launching. These thumbnails are generated from the model data automatically, including new model folders.
- **Random model** sets each model dropdown to **Random model (pick at launch)**.
- **Random personality** sets each personality dropdown to **Random personality (pick at launch)**.
- **Random count (1-10)** checks the per-slot **Random 1-10** box, so that slot chooses a new count at launch.
- **Random all** enables random model, random personality, and random count together.
- **Temperament** is the spider's stable personality. The menu now shows six broad choices: Balanced, Playful, Curious, Bold, Cautious, and Social. Their scheduler values are derived from six transparent 0-10 traits -- energy, curiosity, boldness, sociability, patience, and caution -- where 0 means almost never and 10 means strongly/often. Old specialist personality IDs remain readable in saved presets as legacy entries, but no longer crowd new choices.
- **Job** is separate from temperament and describes colony work. Choose No job, Hunter, Builder, Guard, Scout, or Web tender. Builders establish and upgrade a visible shared team base; Guards patrol it and raise an alert when a declared foe enters its perimeter. Jobs do not silently change personality values.
- **Abilities** opens a per-slot checklist of true capabilities. A fresh slot starts with the temperament's common abilities plus any job capability, and changing temperament/job updates those defaults unless you have edited the list yourself.
- **Colors** is a swatch of the colours that slot will actually produce -- the model's own palette until you override it, and your palette once you have, outlined brightly so an edited row stands out. Click it for the per-slot RGB editor: body, leg, highlight, eye, band, shadow, and tip colours, with a reset to the selected model's defaults. The override is saved in the preset and applies to every creature spawned from that slot.
- The **colour swatch** and the **remove** cross at the end of each row are icons rather than words, because in a row that already carries a model, a temperament, a count, abilities, a team and a job, those two labels were the least informative things in it. Both keep a tooltip and an accessible name, so a screen reader still announces them.
- **Team** assigns a launch-time team to the whole slot, chosen by the name you gave it. Spiders sharing a team are friends by default, and the **Teams** panel below the table is where teams are named, coloured, and given a stance towards each other. Specific friend/neutral/foe overrides remain available in the right-click inspector after launch. Marking teams as foes does not create combat: a Guard alerts and intercepts, and nothing takes damage.
- **Size** offers Tiny, Small, Normal, Large, and Huge launch sizes.
- **Draggable / interferable** toggles whether spiders can be grabbed. When unchecked, clicks pass through spider pixels too.

Changes apply live. While the overlay is running you can pick different models, personalities, counts, skills, or settings and press **Save**: the running overlay reloads the new lineup in place without being stopped or restarted. **Save and launch overlay** does the same when an overlay is already up, so neither button asks you to stop first. Use **Stop overlay** to close it.

**Stop overlay** now asks the overlay to save its spiders and quit, rather than
killing it outright. It previously terminated the process, so any XP, names or
base progress earned since the last automatic save was discarded. The settings
window waits up to five seconds and reports whether the overlay saved; if the
overlay is wedged and does not answer, it is still closed, and the status line
says so rather than implying everything was saved.

## Progression, armor, and teams

Each live spider has a separate runtime progression profile. Eating a fly awards
XP exactly once at the catch point; XP advances the spider through a hard cap of
30. Higher levels give bounded size and speed growth and improve health, energy,
armor, and damage. A small data-driven talent tree offers unlockable passive
bonuses using level-up points, while the older personality/launch skills remain
separate behavior permissions.

Right-click a spider and choose **Inspect progression, inventory, and stats**
to see its level, XP bar, resources, combat values, talent tree, and armor. The
inventory contains spider-specific slots such as carapace, abdomen, legs,
pedipalps, and head; equipment gives derived bonuses and adds restrained visual
armor accents. The inspector also lets you assign a team and set a symmetric
friend/neutral/foe relationship with another spider. Relations are descriptive
until a future combat mode explicitly consumes them, so nothing a spider does
can cause damage. A Guard does read them: it raises an alert when a spider it
considers a foe enters its base perimeter, and moves to intercept it.

### There is no combat yet, and "foes" does not create one

This is worth stating plainly, because the words invite the wrong expectation.
Marking two teams as foes means **a Guard notices an intruder near its base,
raises an alert and moves to intercept**. Nothing takes damage, no spider can be
hurt, and no fight can start. Health, armour and damage exist as numbers on the
inspector and are not consumed by anything. Combat is a later piece of work.

### Naming your own teams

A team has a name you choose and a colour. The settings window has a **Teams**
panel: every team your slots use appears there with a colour swatch, an editable
name and how many spiders are on it. "New team..." in a slot's team picker
creates one from a name you type.

The name is what you see everywhere -- the slot picker, the preset summary and
the right-click inspector -- while the id underneath it is what presets and saved
state refer to, so renaming a team never moves a spider off it. Ids are
case-folded, so `Porch guard` and `porch guard` are the same team rather than two.

The colour is visible on the desktop, which is the point: a base ring is drawn in
its team's colour, each member wears a small ring of it on the ground, and a
hovered spider's name label is edged in it. A preset with two teams now looks
like a preset with two teams.

```json
"teams": {
  "pack_a": { "name": "Home colony", "color": "#4fa3d1" },
  "rivals": { "name": "Intruders",  "color": "#d1534f" }
}
```

A team with no entry still works: it gets a name derived from its id and a colour
derived from it too, so an older preset keeps running and still looks right.

### What stands between two teams

Spiders on the same team are friends and two ordinary teams simply ignore each
other. **Rivals** is the exception: it is hostile to every other named team
unless something says otherwise, so choosing it means something without editing
relations pair by pair.

The Teams panel shows every pair of teams and what stands between them, so this
is a choice you make while setting up rather than a block of JSON you discover
afterwards. It is stored in `settings.team_relations`, one direction per pair,
and read in both:

```json
"team_relations": { "pack_a": { "rivals": "foe" } }
```

Choosing **Ignore each other** for a pair is recorded rather than dropped, so it
can override the Rivals default rather than being restored on the next launch.
A friend/neutral/foe choice made in the right-click inspector still outranks
whatever the teams say. Before any of this existed a Guard had nothing to react
to, because two different teams were merely unrelated, and the shipped
**Colony** preset could not demonstrate the behaviour it advertises.

Runtime state is saved atomically in `state/creatures.json` beside the project or
EXE. It stores level, XP, talents, inventory, equipment, names, team, relations,
the optional pinned level label, and Builder/Guard base progress. Transient
animation and movement state is intentionally not persisted. Launch presets keep
model, temperament, job, team, abilities, colors, and global settings; they do
not contain live HP/energy, animation state, or base build progress. Old presets
continue to work unchanged.

Each saved spider is keyed by the preset it belongs to. That key is now
case-insensitive, because Windows treats `Default.json` and `default.json` as
one file, and previously the two spellings built up two separate profiles for
the same spider. The file is upgraded in place the first time a newer build
reads it: keys differing only in case are merged, keeping the higher level, and
keys from the scheme that predated preset scoping are discarded because they
cannot be attributed to any preset. An entry for a spider that has not appeared
for 50 launches is retired, so the file no longer grows forever.

Set `DESKTOP_BUG_STATE_DIR` to keep runtime state somewhere other than beside
the project or EXE. The headless tests set it so that running them cannot
rewrite real saved spiders.

## Where your own presets go

Presets you save are written to your own folder beside the runtime state:

```text
state\presets```

They are read before the ones that ship with the application, so saving a
preset called `Default` gives you your own `Default` without touching the
`default.json` that came with the build. The shipped copy stays where it is,
and the preset list shows one entry per name, yours.

This used to go wrong. The saved filename comes from the preset's *name*, so
`Default` was written to `presets/Default.json` -- the same file as the shipped
`presets/default.json` on Windows, where case does not distinguish two names.
The first **Save** anyone pressed quietly replaced a preset that shipped with
the application. Writing into the shipped folder is now refused outright.

## Logs and reporting a problem

The executable is built windowed, so it has no console: anything printed would
go nowhere, and a failure used to leave you with a frozen or vanished spider
and nothing to send. Both the overlay and the settings window now write to a
rotating log next to the runtime state:

```text
state\logs\desktop-bug.log
```

It records the version, whether the build is packaged, the preset that was
resolved and where, warnings about unreadable models or presets, Qt's own
warnings, and the full traceback of anything that goes wrong. Three older files
are kept and each is capped at 512 KB, so the whole set stays small enough to
attach to a bug report.

An unhandled error no longer takes the app down in silence. It is written to
the log and announced once through a tray message naming the log, and the
overlay keeps running: an oddly behaved spider is recoverable, a disappeared
application is not. A fault that repeats every frame is counted rather than
written out sixty times a second, and you are told about it once.

Run the overlay with `--verbose` for debug-level detail. The version is also in
the settings window title, so it can be quoted without hunting for it.

## Temperament, jobs, and colony bases

The scheduler still chooses temporary action phases (wander, observe, inspect,
play, and so on), but it reads them from temperament values rather than from a
long list of professions disguised as personalities. A job is a separate role:
Builder work advances a shared team `BaseSite` through five upgrade levels,
while Guard work patrols that site's perimeter and reacts only to explicit
`foe` relations. The base layer is intentionally non-combat for now; its
integrity, resources, alert, ownership, and rendering hooks are ready for later
doors, repairs, crafting, and combat systems.

The shipped **Colony** preset is a quick demonstration: one Builder creates a
team base, one Guard patrols it, a Scout ranges around it, and a rival Hunter
provides a separate team/job example.

While the overlay is running, you can also right-click the system-tray icon for live controls:

- **Randomize > Random model** rerolls creature models without changing the count.
- **Randomize > Random personality** rerolls behavior personalities without changing the count.
- **Randomize > Random count (1-10)** picks a new count between 1 and 10.
- **Randomize > Random model + personality + count** rerolls all three at once.
- **Size** offers Tiny, Small, Normal, Large, and Huge live scale options.
- **Mood** sets the emotional baseline for every spider: Auto (per personality), Playful, Cuddly, Curious, or Calm.
- **Social play (spiders play together)** toggles whether spiders seek each other out to chase and tumble.
- **Let spiders web-trap the mouse** toggles whether spiders with the web-trap or wall-pin skill may shoot sticky silk that catches the pointer. On by default; turning it off frees the pointer at once and stops spiders moving your cursor.
- **Draggable / interferable** toggles whether spiders can be grabbed. When unchecked, clicks pass through spider pixels too.

Right-click an individual spider in the overlay to name it and to open **Skills for this spider**, where each skill can be toggled live for that one spider.

You can also launch the overlay engine directly with a preset:

```bat
set PYTHONPATH=%CD%\src
python -m desktop_bug.engine --preset presets\default.json
```

The packaged executable takes the same arguments:

```bat
dist\DesktopBugCompanion.exe --engine --preset presets\colony.json
```

A relative preset path is looked up in the same order everywhere: an editable
copy beside the project or executable first, then the copy bundled inside the
build. So the command above works against a one-file build even though there is
no `presets` folder next to the `.exe`, and dropping an edited `colony.json`
beside the executable overrides the bundled one. Saving a preset always writes
beside the project or executable, never into the bundle, which is discarded
when the app exits.

## Build the Windows executable

Double-click:

```bat
build_exe.bat
```

`DesktopBugCompanion.spec` is the single definition of the build. The batch
file and both GitHub workflows run it rather than repeating its flags, so there
is one place to change and nothing to keep in step.

The build uses PyInstaller one-file mode and writes a single executable:

```text
dist\DesktopBugCompanion.exe
```

Models, personalities and presets are bundled inside it, so the `.exe` can be
moved on its own. There is no folder to keep beside it.

To override bundled data, put an edited copy in a `models`, `personalities` or
`presets` folder next to the executable; those are searched before the bundled
copies.

## GitHub Actions builds and releases

The repository includes Windows workflows under `.github/workflows/`:

- `ci.yml` runs on pushes and pull requests targeting `main`. It compiles the Python sources, validates every model and preset, and performs a PyInstaller build smoke test.
- Both workflows run `tools/run_all_checks.py`, which compiles the sources,
  validates every model and preset, and discovers and runs every
  `tools/*_smoke.py`. A new test is therefore picked up by both without
  editing a workflow.
- `release-windows.yml` runs for version tags such as `v1.0.0`. It refuses a
  tag that disagrees with `__version__` in `src/desktop_bug/__init__.py`, so a
  published build always reports the version its tag claims. It builds the one-file Windows executable with all current creature models, personalities, and presets bundled, then publishes the `.exe` and a ZIP containing the executable and README to a GitHub Release.

To publish a release, push a semantic-version tag:

```bat
git tag v1.0.0
git push origin v1.0.0
```

You can also run **Build and Release Windows EXE** manually from the Actions tab. Enable **Create or update a GitHub Release** and provide a tag such as `v1.0.0`; leaving it disabled produces a downloadable workflow artifact without creating a release.

## Project structure

```text
DesktopBugCompanion/
  README.md
  requirements.txt
  run_dev.bat
  build_exe.bat
  launcher.py

  src/
    desktop_bug/
      __init__.py
      cage.py
      config_ui.py
      creature.py
      desktop_environment.py
      discovery.py
      engine.py
      flies.py
      frame_policy.py
      jobs.py
      logging_setup.py
      manager.py
      math_utils.py
      mood.py
      mouse_webs.py
      overlay_win32.py
      personality_profiles.py
      phase_scheduler.py
      preset_io.py
      profiling.py
      progression.py
      runtime_state.py
      session_control.py
      skills.py
      teams.py
      webs.py

  models/
    <model id>/
      model.json
      assets/            # optional PNGs, for sprite_rig models

  personalities/
    <personality id>.json    # legacy specialist temperaments

  presets/
    chosen-one.json
    colony.json
    default.json
    snowpuff-2.json
    tarantula.json

  tools/
    benchmark.py             # frame cost at 1/5/10/20 spiders
    benchmark_baseline.json  # what a frame cost before any optimisation
    run_all_checks.py        # everything CI runs, in one command
    validate_model.py
    validate_preset.py
    <name>_smoke.py          # 24 headless checks, all run by CI
```

## Add a new creature model

Create a new folder:

```text
models\ant\model.json
models\ant\assets\
```

Restart the settings UI. The model dropdown auto-discovers `models/*/model.json`. No engine-code edits are required.

Each model needs at least:

```json
{
  "id": "ant",
  "display_name": "Ant",
  "base_size": 22,
  "default_personality": "curious",
  "colors": {
    "body": [35, 30, 25],
    "legs": [20, 17, 14],
    "highlight": [70, 60, 50]
  },
  "legs": []
}
```

Every leg entry must include `name`, `side`, `gait_group`, `attach_angle`, `rest_angle`, `reach`, `upper_len`, and `lower_len`. Optional fields such as `attach_forward`, `attach_side`, `rest_forward`, and `rest_side` improve the procedural rig.

The repository includes five additional recolorable procedural variations: `Mini Marble Tarantula` (small), `Velvet Cloud Tarantula` and `Sunset Fuzzball` (fluffy), `Giant Copper Tarantula` (large and fluffy), and `Blue Jewel Tarantula` (sleek and colorful). They use the articulated five-segment leg chain and painted lateral leg connections. Procedural models respond fully to the Colors editor; sprite-rig art keeps its bitmap appearance while still using palette values for generated joints and expressive details.

Validate a model:

```bat
python tools\validate_model.py models\spider\model.json
```

## Add a new personality

Create:

```text
personalities\aggressive.json
```

Restart the settings UI. The personality dropdown auto-discovers `personalities/*.json`. No engine-code edits are required.

Required personality fields include `id`, `display_name`, `speed_multiplier`, `reaction_radius`, `boldness`, and `wander_frequency`. New temperament definitions should also provide a `temperament` object with `energy`, `curiosity`, `boldness`, `sociability`, `patience`, and `caution`, each from 0 to 10. Additional scalar fields tune threat detection, retreat, chase, idle timing, and approach pauses. Specialist labels from older files remain supported as legacy compatibility data; use a separate slot `job` for work such as `builder` or `guard`.

## Presets

Presets live in `presets/` and are editable JSON files:

```json
{
  "name": "Default",
  "slots": [
    {
      "model": "spider",
      "personality": "hunter",
      "job": "hunter",
      "count": 2,
      "count_random": false,
      "colors": {
        "body": [80, 40, 30],
        "legs": [120, 65, 35],
        "highlight": [220, 140, 60]
      },
      "skills": [
        "approach",
        "wander",
        "drift",
        "jump",
        "roll",
        "chase",
        "observe",
        "run_away",
        "prepare_jump_attack",
        "inspect",
        "cuddle",
        "social_play",
        "zoomies"
      ]
    }
  ],
  "settings": {
    "size_scale": 1.0,
    "interferable": true,
    "mood_mode": "auto",
    "social_play": true
  }
}
```

Each slot may include an optional `skills` list. Omitting `skills` means the slot uses its **personality's default abilities** -- the common set every spider shares plus that personality's specialty -- rather than every ability at once, so spiders behave according to their personality. An explicit list overrides that for the slot, an explicit empty list leaves the slot with only passive/idle behaviour, and older presets that omit `skills` simply fall back to personality defaults. The registered skill ids are `approach`, `wander`, `drift`, `jump`, `roll`, `chase`, `observe`, `run_away`, `prepare_jump_attack`, `inspect`, `cuddle`, `social_play`, `zoomies`, `weave_web`, `web_walk`, `shoot_web`, and `wall_web`. Of these, most are common to every spider; the specialist abilities are `weave_web` (Webber), `shoot_web` and `wall_web` (Trapper), and `drift` (Drifter), which are granted only to their matching personality by default but can be added to or removed from any slot.

The `settings` block also accepts an optional `mood_mode` (the same values as the tray Mood menu) and `social_play` flag, so a preset can launch straight into a chosen mood with playing on or off. Both default to `auto` and `true` when omitted, so older presets keep working unchanged.

The settings UI can add/remove slots, pick model/temperament/job/count/abilities/colors/team, save a preset, load a preset, and launch the engine using the selected preset.

Validate a preset:

```bat
python tools\validate_preset.py presets\default.json
```

## How to quit

- Preferred: return to the settings UI and click **Stop Overlay**.
- The overlay process also creates a system-tray icon. Right-click it and choose **Quit overlay**.

## Transparent overlay troubleshooting

The overlay must pass this visual test:

1. Open Word, a browser, or File Explorer.
2. Launch Desktop Bug Companion.
3. Click **Save and launch overlay**.
4. The spider should appear directly on top of the existing application.
5. There should be no black, white, gray, or colored background.
6. You should still be able to click/type in the app underneath.

If you see a black fullscreen rectangle:

- Confirm you are running on Windows; true click-through behavior is Windows-specific in this project.
- Make sure `engine.py` has not been changed to fill the painter background. It should not call `fillRect` for the overlay background.
- Confirm `Qt.WA_TranslucentBackground` is set and `overlay_win32.apply_click_through()` is being called after `show()`.
- Some screen recorders, remote desktops, or GPU overlay tools interfere with per-pixel transparency. Test on a normal local desktop.
- Rebuild after dependency changes with `build_exe.bat`.

## Acceptance checklist

- Transparent click-through overlay, always on top.
- Spider pixels are left-draggable while empty overlay space still passes clicks through.
- Drag release preserves throw inertia and triggers a startled visual/scurry response.
- Main settings window and tray menu can randomize model/personality/count, change size, and toggle draggable/interferable mode.
- No visible canvas or background.
- Spider body has abdomen, cephalothorax, eyes/pedipalps, and 8 data-driven legs.
- Legs use planted-foot IK and alternating gait groups.
- FSM has Idle, Alert, Approach, Chase, Retreat, and Wander, plus expressive Aim, Coil, Jump, Land, Catch, Feed (devouring a fly), Inspect, Cuddle, Play, Zoom, Roll, and drift-style movement, including autonomous Drifter charge-up/circle/corner momentum slides, the web-weaving WeaveApproach, Weave, WebApproach, and WebWalk states, the web-repair RepairApproach and Repair states, and the cursor-trapping WebAim and WebShot states. These abilities are gated by explicit skill classes in `src/desktop_bug/skills.py`. Flies and the spiders' hunting of them live in `src/desktop_bug/flies.py`.
- Spiders express mood through antenna shape, eye openness and gaze, and body wiggle, nod, crouch, rear, and abdomen wag.
- The body can wiggle and shift while planted feet stay put, so motion never reads as a crab walk.
- Bold spiders crouch, range, and pounce at a target, then resolve into cuddle, run-away, or catch on landing.
- With more than one spider and social play on, spiders seek each other out to chase and tumble.
- Tray Mood menu overrides the emotional baseline; Social play toggle enables or disables spider-to-spider play.
- Playful and Cuddly personalities ship in the box.
- Approach and Retreat set their targets immediately.
- Fast cursor movement toward the spider triggers real retreat.
- Observer movement backs away along the real opposite vector from the watched cursor/spider instead of choosing only left/right orbit sides.
- Added the **Nope** personality for rapid backward zigzag escape jumps when the mouse approaches.
- Config UI edits preset slots and launches the overlay.
- Models/personalities/presets are auto-discovered and editable after build.
- Right-clicking a spider names or renames it; hovering a named spider shows an upright label; tray and right-click menus can always-show every name. The same right-click menu can toggle skills for that individual live spider.
- Cages can be added, moved by their border, and resized by their corners; spiders inside are confined yet can be dragged out, and several cages can hold their own spiders at once.
- The overlay repaints only the changing region (spiders plus cage borders) instead of the whole screen, with off-region spiders skipped.
- A stopped spider holds its feet still instead of re-homing to a rest pose or twitching; the hunter stays frozen while watching a still cursor.
- The hunter approaches and chases the cursor noticeably faster than other spiders.
- Moving or resizing a cage repaints its whole footprint so no translucent ghost is left at the old position.
- Playful, excited spiders sometimes tuck in and roll, spinning through a turn or two before resuming.
- Spiders weave silk webs thread by thread following a real orb-weaver build order (bridge, frame, radii, hub, temporary outward spiral, then sticky inward capture spiral), in four finished forms (corner orb, full orb, funnel sheet, cobweb tangle). The **Webber** personality builds them, favouring corners but also placing webs in open spots; other spiders walk onto finished webs and pluck them to test the bounce; and any spider may adopt and finish an abandoned, half-built web it did not start. The **Webber** personality ships in the box, gated by the `weave_web` and `web_walk` skills.
- A Webber keeps at most seven webs on screen and builds less eagerly as more intact webs already exist, so it spins a handful and then settles rather than carpeting the desktop.
- Dragging the mouse pointer across a finished web tears the strands it passes over, breaking the web in parts where you swipe rather than all at once; a web only tears while the pointer is moving over it. A Webber notices a torn web, travels to it, and re-knits the missing strands until it is whole again (RepairApproach and Repair states).
- Spiders are not all able to do everything: a common set of abilities is shared by every personality, while specialist abilities (`weave_web` for the Webber, `shoot_web`/`wall_web` for the Trapper, `drift` for the Drifter) belong to their matching personality by default. A slot with no explicit `skills` uses its personality's defaults, and any specialist ability can still be added to or removed from a slot.
- Spiders can shoot sticky silk at the real pointer: `shoot_web` pins the cursor in place and `wall_web` shoves it to the nearest wall, both broken by wiggling the mouse, with a struggle meter that drains when still and a hard maximum hold so the pointer is never locked. The **Trapper** personality stalks and webs the cursor, a tray **Interaction** toggle (**Let spiders web-trap the mouse**, on by default) is the master switch, and pointer control is Windows-only and degrades safely if it fails.
- The running overlay applies edits live: saving the preset (or relaunching) while it runs reloads new models, personalities, counts, skills, and settings in place, without stopping or restarting the overlay. The overlay watches its launched preset file and reloads it when it changes, skipping a half-written file safely.


## Leg orientation update

Leg targets are now constrained in the spider's local body space instead of drifting freely in world space. This means:

- left legs stay on the left side of the body,
- right legs stay on the right side of the body,
- foot targets are clamped to a heading-relative envelope,
- turning the body causes the legs to re-home based on the spider's facing direction.

This specifically addresses the issue where some legs could look too independent and end up pointing in unrealistic directions unrelated to the body orientation.


## Anatomical leg-solver hotfix

The leg renderer now uses a stable body-local knee solver. Foot targets are still used for stepping, but before rendering each leg is hard-clamped to its correct body-side and front/mid/rear lane. The knee is also solved in body-local space so it always bows outward from the spider instead of flipping across the body during turns. This fixes the visual case where leg pairs could appear rotated to the opposite side of the direction the spider is facing.

## Anti-crab gait update

The latest locomotion pass fixes a problem introduced by the stricter anatomical leg solver: planted feet were being clamped to the moving body every frame, which made the spider look like it was side-stepping like a crab.

The gait now works more like a spider:

- planted feet stay in world/screen space while the body moves over them,
- only step targets are constrained into safe body-local lanes,
- visual correction is applied only when a foot becomes severely impossible after a sharp turn,
- swing arcs move feet forward relative to the spider's facing direction,
- body sway from turns was reduced so the motion reads as forward crawling instead of lateral sliding.

This keeps the anatomical safety from the previous update while restoring a more spider-like planted-foot walk.


## Smooth performance tuning

This build defaults to smooth 60 FPS again, but throttles the expensive non-visual work so it does less wasted CPU work at high FPS. Right-click the tray icon and use **Performance** to switch between 60, 45, 30, and 20 FPS while it is running.

For the smooth look, use **Smooth 60 FPS** with fewer spiders or smaller spider size. If the desktop starts lagging after a while, try **Still smooth 45 FPS** before dropping to 30 FPS. You can also launch with an FPS override:

```bat
set DESKTOP_BUG_FPS=60
DesktopBugCompanion.exe
```

Leave `DESKTOP_BUG_FAST_PIXMAPS` off unless you are testing speed and can accept lower quality sprite rendering.

### Measuring, instead of guessing

Until recently nothing in the frame loop was timed, so every explanation for why
a larger colony feels heavy was a hunch. It is now measured.

Set `DESKTOP_BUG_PROFILE=1` to time each system per frame, and a small HUD
appears in the top-left corner showing the frame cost, the costliest systems and
the rate the overlay is running at. `DESKTOP_BUG_PROFILE_HUD=0` keeps the timing
and hides the panel. Both are off by default.

Leaving the instrumentation in the code costs about 0.07 ms per frame with ten
spiders when profiling is off, which is smaller than the run-to-run variation of
the measurement itself, so treat it as free rather than as zero. Switching it on
costs about 0.4 ms per frame at that colony size.

To measure without running the overlay at all:

```bat
python tools/benchmark.py
```

It runs the real manager and the real painter headless at 1, 5, 10 and 20
spiders and prints where the time goes. `--check` compares against
`tools/benchmark_baseline.json` and fails if a colony size got more than 15 %
slower; `--update-baseline` records a new one. A baseline only applies to the
machine it was recorded on, so it is compared against a hardware fingerprint and
skipped rather than failed elsewhere.

**What the measurement found.** Drawing the spiders is about four fifths of a
frame and scales linearly with the colony. Ten spiders cost about 23 ms per
frame against a 16.7 ms budget at 60 FPS, which is why ten was where it started
to stutter. Simulating them is only about a fifth of that. Webs, flies, jobs,
behaviour scheduling and desktop probing together account for under 0.05 ms,
and the repaint region at ten spiders still covers only about 16 % of the
screen, so this was never a pixel-count problem and never a behaviour-scheduling
one.

Profiling inside the drawing found the cost was not Qt but Python recomputing
answers it already had. The gait tuning -- a thirty-key table of bounded values
read straight out of the model -- was rebuilt about **seventy-two times per
spider per frame**. The heading's forward and right vectors, four trigonometric
calls, were recomputed about **460 times per spider per frame**. Every leg chain
was solved **twice**: once for the leg, once for the sockets and knuckles drawn
over it. For a planted leg those two are identical, which is most legs most of
the time; for one mid-swing they are not, because the leg pass raises the foot
before solving, so that one stays two real solves.

Caching those, plus the palette colours and the leg reach limits, made a frame
about **21 % cheaper** with no change at all to what is drawn:

| spiders | before | after |
| --- | --- | --- |
| 1 | 2.23 ms | 1.75 ms |
| 4 | 8.99 ms | 7.09 ms |
| 10 | 22.11 ms | 17.56 ms |
| 20 | 44.36 ms | 35.26 ms |

Both columns come from one paired run on the same machine, with the old and new
`creature.py` swapped in turn, because two measurements taken minutes apart vary
by a few percent and a mixed table would flatter the result.

Ten spiders now run at roughly 57 FPS rather than 45. That is close to the
16.7 ms budget but not inside it, so a large colony on a slower machine will
still drop frames; the remaining cost is the draw calls themselves and the leg
solver, and reducing those means either batching the drawing or simplifying how
a spider looks when several are on screen.

"Faster" here means *only* faster: the identity is checked by rendering the same
seeded run twice, once with every cache disabled, and comparing the images pixel
by pixel.

### Frame rate that follows the machine

A desktop pet painting at 60 FPS behind a fullscreen game, or on a laptop
running off its battery, is being a bad guest. The overlay now drops to 10 FPS
while a fullscreen window has the foreground, and to 30 FPS on battery, and
returns to the rate you chose when that stops being true. The tray **Performance**
menu sets the ceiling it works from, so your choice is remembered rather than
overwritten.

### Partial-repaint efficiency

The overlay is a transparent window the size of the whole virtual desktop. Earlier builds repainted that entire surface on every frame, which is the main reason a busy desktop could feel heavy even with only a few spiders, since most of the repainted area was empty transparent pixels.

This build repaints only the parts of the screen that actually change. Each frame it gathers the bounding box of every spider, the band a spider just vacated so fast throws do not leave a streak, and the thin border of every cage, then asks the window to repaint just that combined region. The frame rate is unchanged, so the motion stays as smooth as before and nothing is clipped, but the painted area collapses from the full screen to the footprints of the spiders and cages. In a typical scene that is a few percent of the screen instead of all of it. Inside the manager each spider is also skipped entirely when it falls outside the repaint region, so off-screen work is avoided as well.

The effect is largest when spiders are small or few and the desktop is large. With many large spiders spread across the screen the repaint region naturally grows, so the older FPS controls above still matter for the heaviest scenes.
