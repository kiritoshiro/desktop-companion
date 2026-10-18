# dc-world port status

| Python module | Rust | Status |
|---|---|---|
| `world/playfield.py` | `src/playfield.rs` | **ported**: `ScreenRect`, `Playfield` (fast path, margin, seam clamp), `rects_from_geometry` |
| `world/screen_layout.py` | `src/screen_layout.rs` | **ported**: `Screen`, `ScreenLink` (`LinkKind::Door`/`Tunnel`), `ScreenLayout` (`new`, `single`, `screen_at`, `clamp`, `links_of`, `tunnels`, `path`, `route`, `tunnel_at`), doors, tunnels |
| `world/tactics.py` | `src/tactics.rs` | **ported**: `Member<K>`, `Foe<K>`, `Order<K>`, `Plan<K>`, `plan`, `choose_focus`, `tactics_level`; all doctrines |
| `world/aimed_silk.py` | — | **not started** (blocked, see below) |
| `world/webs.py`, `flies.py`, `mouse_webs.py`, `jobs.py`, `cage.py`, `carcass.py`, `desktop_environment.py` | — | not started (wave 2, other agent) |

## Notes

- Python strings that name a closed set are enums with `as_str()`/`parse()`:
  `Role`, `OrderMode`, `Doctrine`, `Stance`, `LinkKind`.
- Tactics keys are generic (`K: Clone + PartialEq`): the mission can pass actor
  ids. `Plan::orders` is a `Vec<(K, Order<K>)>` in Python's dict insertion order;
  look up with `Plan::order(&key)`.
- `ScreenLayout::new` returns `Err(EmptyLayout)` where Python raises `ValueError`.
- Identity checks (`other is rect`, `m is not bait`) became index checks.
- `tunnel_at(x, y, reach)` takes the reach explicitly; Python's default is
  `screen_layout::TUNNEL_REACH`.

## aimed_silk (blocked)

`AimedSilk` subclasses `flies.WebShotProjectile` and calls
`flies._pin_fly_with_shot`, and per frame reads from creatures `web_pinned`,
`dead`, `webbed`, `relation_to(shooter)`, `airborne`, `size`, `x`, `y`, and
from flies `alive`, `eaten`, `trapped`, `x`, `y`, `size`. It needs the `flies`
module (`WebShotProjectile`, `WEB_SHOT_SPEED`, `_pin_fly_with_shot`) and a
creature view; port it alongside `flies.rs`. The swept hit test itself is 20
lines (closest point on the step segment, radius `max(13, size * 0.7)`,
earliest `t` wins). `dc_adventure::adventure::SilkShot { angle, reach }` is
what the player controller hands over to build one.

## Tests

- Unit tests port the geometry tests of `tests/test_two_screens.py`, the screen
  tests of `tests/test_screens_and_reclaim.py`, and the squad tests of
  `tests/test_enemy_tactics.py`.
- `tests/python_parity.rs` compares against fixtures from
  `tools/dump_fixtures.py` (run with the project venv, `python -I`): 600 random
  tactics plans (every doctrine), 22 layouts with routes/clamps/paths/tunnels,
  and 1350 playfield contains/clamp queries. Floats agree to 1e-9 relative.
