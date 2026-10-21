-- Requires the private_overlay patch for Hyprland c7216a9807a2683ce243f6ed14ae01e7e884c426.
-- Load after Lennette's generated shortcut module. An unpatched compositor must
-- reject this rule: a normal float/pin rule does not provide capture exclusion.
hl.window_rule({
	name = "lennette-private-overlay",
	match = { class = [[^(dev\.lennette\.overlay|lennette)$]] },
	float = true,
	pin = true,
	border_size = 0,
	rounding = 0,
	private_overlay = true,
	no_blur = true,
})
hl.window_rule({
	name = "lennette-overlay-size",
	match = { class = [[^dev\.lennette\.overlay$]], title = "^lennette$" },
	size = { 720, 620 },
	center = true,
})
hl.window_rule({
	name = "lennette-dashboard-size",
	match = { class = [[^dev\.lennette\.overlay$]], title = "^lennette dashboard$" },
	size = { 1100, 760 },
	center = true,
})
