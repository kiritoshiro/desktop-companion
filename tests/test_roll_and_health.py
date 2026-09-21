"""A spin must end cleanly however it ends, and a health bar must be pinnable.

Two things reported from play. A spider's legs looked wrong after it spun, and
there was no way to keep a health bar on screen.

The legs had two separate causes, and both are measured here rather than
described. A roll turned a random *fraction* of a turn while the legs re-planted
against the unchanged logical heading, so the body jumped by up to 173 degrees
on the last frame. And only a roll that ran to completion cleaned up after
itself, so anything that cut one short -- a startle, a grab, a job, the skill
being switched off -- left the body rotated by whatever the spin had reached and
the legs still tucked, permanently.
"""

import json
import math
import random



from desktop_bug.creature import Creature
from desktop_bug.state.progression import ProgressionState
from support import ROOT
import pytest


@pytest.fixture(autouse=True, scope="module")
def _qt(qapp):
    """Every check in this module needs the one Qt application object.

    Each of these files used to build its own, and several dropped the only
    reference to it on the same line. In one process per test that was merely
    wasteful; in one process for the whole suite it is an access violation,
    because the next module inherits a pointer to an application that has
    already been collected. `conftest.qapp` owns it now.
    """

DT = 1.0 / 60.0
SCREEN = (1600, 900)
AWAY = (-100000.0, -100000.0)
SEEDS = (1, 7, 11, 23, 42, 99, 123)

def qt_app():
    """The one application object, owned by the `qapp` fixture in conftest.

    It used to be created here, and the two ways of getting that wrong are
    written up in conftest: dropping the only reference on the same line, and
    creating a QGuiApplication that then makes every widget check in the
    process abort.
    """
    from PyQt5.QtWidgets import QApplication

    app = QApplication.instance()
    assert app is not None, "the qapp fixture has not run; nothing owns the application"
    return app


def build(seed: int, personality: str = "playful") -> Creature:
    random.seed(seed)
    model = json.loads((ROOT / "models" / "tarantula" / "model.json").read_text(encoding="utf-8"))
    traits = json.loads((ROOT / "personalities" / f"{personality}.json").read_text(encoding="utf-8"))
    creature = Creature(model, traits, *SCREEN, index=0, progression_id=f"roll:{seed}")
    creature.x, creature.y = 800.0, 450.0
    creature._initialize_legs()
    for _ in range(60):
        creature.update(DT, *AWAY, *SCREEN)
    # A playful spider tumbles on its own, which would put a spontaneous roll in
    # the middle of a measurement of a deliberate one. Take the skill away and
    # hand it back only when the test wants it.
    creature.set_skill_enabled("roll", False)
    for _ in range(10):
        creature.update(DT, *AWAY, *SCREEN)
    return creature


def start_roll(creature: Creature) -> bool:
    creature.set_skill_enabled("roll", True)
    creature.enter_roll()
    return creature.state == "Roll"


def worst_reach_ratio(creature: Creature) -> float:
    worst = 0.0
    for leg in creature.legs:
        ax, ay = creature._leg_attach(leg)
        reach = creature._leg_max_reach(leg, visual=True)
        worst = max(worst, math.hypot(leg.foot_x - ax, leg.foot_y - ay) / max(1e-6, reach))
    return worst


def assert_tidy(creature: Creature, label: str) -> None:
    assert not creature._rolling, f"{label}: the spider still believes it is rolling"
    assert abs(creature.roll_spin) < 1e-9, (
        f"{label}: {math.degrees(abs(creature.roll_spin)):.0f} degrees of rotation "
        "was left applied to the body"
    )
    assert abs(creature.roll_tuck) < 1e-9, f"{label}: the legs were left tucked in"
    ratio = worst_reach_ratio(creature)
    assert ratio < 1.15, f"{label}: a foot ended up at {ratio:.2f} times its own reach"


def test_a_finished_spin_does_not_snap() -> None:
    """The body has to end facing the way its legs are about to re-plant."""
    worst_snap = 0.0
    rolled = 0
    for seed in SEEDS:
        creature = build(seed)
        if not start_roll(creature):
            continue
        rolled += 1
        # Whole turns, decided at the start, so the end is knowable up front.
        turns = creature.roll_total / math.tau
        assert abs(turns - round(turns)) < 1e-9, (
            f"a roll was planned as {turns:.2f} turns; a fraction of a turn ends "
            "with the body jumping to catch up with its own legs"
        )
        last = 0.0
        for _ in range(400):
            previous = creature.roll_spin
            creature.update(DT, *AWAY, *SCREEN)
            if creature.state != "Roll":
                last = previous
                break
        # How far the drawn body moves on the frame the spin is dropped to zero.
        snap = abs((last % math.tau + math.pi) % math.tau - math.pi)
        worst_snap = max(worst_snap, snap)
        assert_tidy(creature, f"seed {seed} after a finished spin")
    assert rolled >= 5, f"only {rolled} spiders rolled, so this proves little"
    assert math.degrees(worst_snap) < 1.0, (
        f"a finished spin left the body {math.degrees(worst_snap):.0f} degrees "
        "away from where it started"
    )


def interrupt_and_settle(seed: int, interrupt) -> Creature:
    creature = build(seed)
    if not start_roll(creature):
        return None
    for frame in range(400):
        creature.update(DT, *AWAY, *SCREEN)
        if frame == 18:
            interrupt(creature)
            break
        if creature.state != "Roll":
            return None  # finished before it could be interrupted
    for _ in range(20):
        creature.update(DT, *AWAY, *SCREEN)
    return creature


def test_every_way_out_of_a_spin_tidies_up() -> None:
    """A state can be left in more ways than it can be finished."""
    ways = {
        "startled": lambda c: c.enter_startled(900.0, 500.0),
        "grabbed": lambda c: c.start_drag(820.0, 470.0),
        "skill switched off": lambda c: c.set_skill_enabled("roll", False),
        "sent somewhere else": lambda c: c.enter_idle(),
    }
    for label, interrupt in ways.items():
        tested = 0
        for seed in SEEDS:
            creature = interrupt_and_settle(seed, interrupt)
            if creature is None:
                continue
            tested += 1
            assert_tidy(creature, f"seed {seed}, {label} mid-spin")
        assert tested >= 5, f"{label}: only {tested} spiders were interrupted"


def test_a_dragged_spider_is_tidied_before_it_is_drawn() -> None:
    """A grab makes `update` return early, so the tidy-up has to come first.

    Takes the first seed that is still mid-spin rather than assuming a fixed
    number of frames. A spider can be pulled out of its own tumble by the phase
    scheduler, and how often that happens shifts with Python's per-process
    string hashing -- the same flakiness that caught the throw test in DC-31.
    """
    for seed in SEEDS:
        creature = build(seed)
        if not start_roll(creature):
            continue
        for _ in range(40):
            creature.update(DT, *AWAY, *SCREEN)
            if creature.state != "Roll":
                break
            if abs(creature.roll_spin) > 0.5:
                creature.start_drag(820.0, 470.0)
                creature.update(DT, 820.0, 470.0, *SCREEN)
                assert_tidy(creature, f"seed {seed}, the first frame of being carried")
                return
    raise AssertionError("no spider stayed in a spin long enough to be grabbed")


def test_no_frame_ever_ends_mid_tidy() -> None:
    """The invariant, checked where it matters: the moment a frame is drawn.

    Tidying up at the top of the next frame is not enough on its own. The phase
    scheduler really does pull spiders out of their own tumbles -- that is what
    made an earlier version of the grab check flaky -- and when it does, the
    frame in between would be drawn with the body still rotated. So this runs
    spiders that tumble on their own and asserts, after every single update,
    that a spider which is not rolling is not spun and not tucked.
    """
    rolls_seen = 0
    for seed in SEEDS:
        creature = build(seed)
        creature.set_skill_enabled("roll", True)
        was_rolling = False
        for frame in range(900):
            creature.update(DT, *AWAY, *SCREEN)
            if creature.state == "Roll":
                was_rolling = True
                continue
            if was_rolling:
                rolls_seen += 1
                was_rolling = False
            assert abs(creature.roll_spin) < 1e-9, (
                f"seed {seed}, frame {frame}: a frame would have been drawn with "
                f"{math.degrees(abs(creature.roll_spin)):.0f} degrees of leftover spin"
            )
            assert abs(creature.roll_tuck) < 1e-9, (
                f"seed {seed}, frame {frame}: a frame would have been drawn with "
                "the legs still tucked"
            )
    assert rolls_seen >= 3, (
        f"only {rolls_seen} tumbles happened on their own, so the invariant was "
        "barely exercised"
    )


# ----------------------------------------------------------------------
# The pinned health bar
# ----------------------------------------------------------------------

def test_the_pin_is_remembered() -> None:
    state = ProgressionState()
    assert state.pin_health is False
    state.pin_health = True
    restored = ProgressionState.from_dict(state.to_dict())
    assert restored.pin_health is True, "a pinned health bar was not saved"
    assert ProgressionState.from_dict({}).pin_health is False


def test_pinning_shows_the_label_on_its_own() -> None:
    creature = build(5)
    assert creature.label_visible(False) is False, "an unnamed spider showed a label"
    creature.set_health_label_pinned(True)
    assert creature.health_label_pinned is True
    assert creature.label_visible(False) is True, (
        "pinning the health bar did not bring the label on screen, so there is "
        "nowhere for the bar to be"
    )
    creature.set_health_label_pinned(False)
    assert creature.label_visible(False) is False


def test_the_bar_is_drawn_and_follows_the_health() -> None:
    """Read the pixels rather than trusting that a colour was passed to a brush."""
    from PyQt5.QtGui import QColor, QImage, QPainter

    qt_app()

    def paint(pinned: bool, fraction: float) -> QImage:
        creature = build(9)
        creature.set_health_label_pinned(pinned)
        creature.hp = creature.max_hp * fraction
        image = QImage(600, 600, QImage.Format_ARGB32_Premultiplied)
        image.fill(QColor(0, 0, 0, 0))
        painter = QPainter(image)
        painter.setRenderHint(QPainter.Antialiasing, True)
        creature.x, creature.y = 300.0, 380.0
        creature.render(painter, always_show_names=True)
        painter.end()
        return image

    def run_of(image: QImage, target) -> int:
        """Widest horizontal run of the bar colour, anywhere above the body."""
        widest = 0
        for y in range(180, 380):
            run = 0
            for x in range(120, 480):
                pixel = image.pixelColor(x, y)
                close = (pixel.alpha() > 150
                         and math.dist((pixel.red(), pixel.green(), pixel.blue()), target) < 40)
                run = run + 1 if close else 0
                widest = max(widest, run)
        return widest

    green = (104, 194, 108)
    red = (214, 84, 76)

    full = run_of(paint(True, 1.0), green)
    assert full >= 20, f"no health bar was drawn: widest green run {full}px"

    unpinned = run_of(paint(False, 1.0), green)
    assert unpinned < 8, f"an unpinned spider drew a health bar anyway: {unpinned}px"

    hurt = run_of(paint(True, 0.25), red)
    assert hurt >= 5, f"a hurt spider drew no red bar: {hurt}px"
    assert hurt < full * 0.6, (
        f"the bar did not shorten with the health: {hurt}px at a quarter health "
        f"against {full}px at full"
    )
    assert run_of(paint(True, 0.25), green) < 8, "a quarter-health spider still read as healthy"


def test_the_bar_is_inside_the_repaint_footprint() -> None:
    """Outside it, the bar would smear instead of updating."""
    creature = build(11)
    creature._hovered = True
    creature.set_name("Boris")
    plain = creature.bounding_rect(True)
    creature.set_health_label_pinned(True)
    pinned = creature.bounding_rect(True)
    assert pinned[1] < plain[1] - 2.0, (
        "the bounding box did not grow upward for the bar, so a partial repaint "
        f"would clip it: {plain[1]:.1f} -> {pinned[1]:.1f}"
    )


def test_the_colour_says_what_the_number_says() -> None:
    from PyQt5.QtGui import QColor

    creature = build(13)
    assert creature.health_fraction() == 1.0
    creature.hp = creature.max_hp * 0.5
    assert abs(creature.health_fraction() - 0.5) < 1e-6
    creature.hp = -50.0
    assert creature.health_fraction() == 0.0, "health below zero should read as empty"

    healthy = Creature.health_bar_color(1.0, QColor)
    hurt = Creature.health_bar_color(0.45, QColor)
    dying = Creature.health_bar_color(0.1, QColor)
    assert healthy.green() > healthy.red(), "full health did not read as green"
    assert dying.red() > dying.green(), "near-death did not read as red"
    assert hurt.red() != healthy.red() and hurt.red() != dying.red(), (
        "the middle of the range is not distinguishable from either end"
    )
