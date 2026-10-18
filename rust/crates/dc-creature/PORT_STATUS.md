# dc-creature port status

(Draft: modules, contract and snapshot are final; tests and parity results are
being filled in.)

Simulation half of `src/desktop_bug/creature/`. One [`Creature`] per spider:
`creature.kin` is its `dc_kinematics::Kinematics` (pose, legs, gait, roll,
drift, throw inertia); every other Python attribute is a plain field named
without its leading underscore.

## Per-frame contract (for dc-manager)

```rust
let mut cmds = Vec::new();
let mut env = Env { population: &pop, webs: Some(&mut web_world), flies: Some(&mut fly_world),
                    mouse_webs: Some(&mut mouse_webs), bases: Some(&base_world), commands: &mut cmds };
match creature.update(dt, mx, my, screen_w, screen_h, &mut env) {
    UpdateOutcome::Ran | UpdateOutcome::Dead => {}
    UpdateOutcome::PlayerControlled => controller.update(dt, &mut creature /* ControlledSpider */),
}
pop.set(creature.view());                    // later creatures see the new state
for c in cmds.drain(..) { target.apply_command(&c, &pop) }   // play tags, mood bumps
```

See `PORT_STATUS.md` (final) for the full list of inputs.
