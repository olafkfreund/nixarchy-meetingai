import { createFileRoute } from "@tanstack/react-router";

import { Command, CommandList } from "@/components/command";
import { GITHUB_URL, PageShell, RELEASES_URL } from "@/components/page-shell";
import { Tabs } from "@/components/tabs";

const Step = ({
  index,
  title,
  children,
}: {
  index: number;
  title: string;
  children: React.ReactNode;
}) => (
  <section className="border-t border-ink/10 pt-10">
    <div className="flex items-baseline gap-4">
      <span className="font-display text-2xl text-sky-deep">
        {String(index).padStart(2, "0")}
      </span>
      <h2 className="font-display text-[clamp(1.6rem,3vw,2.2rem)] leading-tight text-ink">
        {title}
      </h2>
    </div>
    <div className="mt-5 flex flex-col gap-4 text-ink-soft [&>p]:max-w-2xl [&>p]:leading-relaxed">
      {children}
    </div>
  </section>
);

const Code = ({ children }: { children: string }) => (
  <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">{children}</code>
);

const FLAKE_LINE =
  'inputs.lennette-private.url = "github:parazeeknova/lennette?dir=packaging/hyprland";';
const HYPR_COMMIT = "c7216a9807a2683ce243f6ed14ae01e7e884c426";
const INSTALL_CMD = "devenv shell -- just install";
const TTY_CMD = "target/hyprland-private/Hyprland";
const RULE_LOAD =
  'dofile("/absolute/path/to/lennette/packaging/hyprland/lennette-private.lua")';

const bindings = [
  { keys: "Ctrl+Shift+D", action: "Open the dashboard" },
  { keys: "Ctrl+Shift+Backslash", action: "Show or hide the overlay window" },
  { keys: "Ctrl+Shift+I", action: "Focus the prompt input" },
  { keys: "Ctrl+Shift+M", action: "System audio command" },
  { keys: "Ctrl+Shift+A", action: "Voice input command" },
  { keys: "Ctrl+Shift+S", action: "Screenshot command" },
];

const NixosSteps = () => (
  <div className="flex flex-col gap-12">
    <Step index={1} title="Add the flake input">
      <p>
        Point your system flake at the packaging directory. This brings in the
        patched compositor, the Home Manager module and the launcher.
      </p>
      <Command label="flake.nix">{FLAKE_LINE}</Command>
    </Step>

    <Step index={2} title="Enable the modules">
      <p>
        Import both modules and enable the integration. Replace{" "}
        <Code>alice</Code> with your own Home Manager user.
      </p>
      <Command label="configuration.nix">
        {`{ inputs, ... }:
{
  imports = [ inputs.lennette-private.nixosModules.default ];
  programs.lennettePrivateOverlay.enable = true;

  home-manager.users.alice = {
    imports = [ inputs.lennette-private.homeManagerModules.default ];
    programs.lennettePrivateOverlay.enable = true;

    # Use the NixOS compositor package for the login session.
    wayland.windowManager.hyprland.package = null;
  };
}`}
      </Command>
    </Step>

    <Step index={3} title="Build and switch">
      <p>
        Rebuild before activation. Replace <Code>my-host</Code> with the host
        name from your own flake.
      </p>
      <CommandList
        commands={[
          "nix flake update lennette-private",
          "nixos-rebuild build --flake .#my-host",
          "sudo nixos-rebuild switch --flake .#my-host",
        ]}
      />
      <p className="text-ink">
        A switch alone does not replace the running compositor. Save your work,
        log out of Hyprland, and log back in.
      </p>
    </Step>

    <Step index={4} title="Verify, then launch">
      <p>
        After logging back in, check the session is compatible before starting
        the app. The launcher refuses to run on a compositor that does not
        accept the protection rule.
      </p>
      <CommandList
        commands={["lennette-private-user --check", "lennette-private-user"]}
      />
    </Step>

    <Step index={5} title="Install the app itself">
      <p>
        The compositor is separate from the application. From a Lennette
        checkout, install the binary and make sure <Code>~/.local/bin</Code> is
        on your <Code>PATH</Code>.
      </p>
      <Command label="from a checkout">{INSTALL_CMD}</Command>
      <p>
        Quit any running Lennette instance before using private mode. A desktop
        entry named <span className="text-ink">Lennette (private overlay)</span>{" "}
        is added, and the ordinary Lennette entry is routed through the
        protected launcher.
      </p>
    </Step>
  </div>
);

const DistroSteps = () => (
  <div className="flex flex-col gap-12">
    <Step index={1} title="Install build dependencies">
      <p>
        The compositor is built from the pinned Hyprland source, so you need its
        toolchain plus the patch prerequisites. Package names below are for Arch
        and Ubuntu; adjust for Fedora or openSUSE.
      </p>
      <Command label="Arch Linux">
        {`sudo pacman -S --needed base-devel git cmake ninja pkgconf wayland wayland-protocols \\
  hyprwayland-scanner libdrm mesa libxkbcommon pixman libinput libxcb \\
  xcb-util xcb-util-wm xcb-util-errors cairo pango libjpeg-turbo libwebp \\
  vulkan-icd-loader lua jq`}
      </Command>
      <Command label="Ubuntu / Debian">
        {`sudo apt install build-essential git cmake ninja-build pkg-config \\
  libwayland-dev wayland-protocols libdrm-dev libgbm-dev libegl-dev \\
  libxkbcommon-dev libpixman-1-dev libinput-dev libxcb1-dev libxcb-util-dev \\
  libxcb-composite0-dev libcairo2-dev libpango1.0-dev libjpeg-dev libwebp-dev \\
  libvulkan-dev liblua5.4-dev jq`}
      </Command>
      <p>
        You also need <Code>hyprwayland-scanner</Code>, which most distributions
        do not package. Build it from source if the step below cannot find it.
      </p>
    </Step>

    <Step index={2} title="Check out the pinned Hyprland revision">
      <p>
        The patch only applies to one commit. Cloning anything else fails the
        build script on purpose.
      </p>
      <Command label="git">
        {`git clone --recursive https://github.com/hyprwm/Hyprland ~/Hyprland
git -C ~/Hyprland checkout ${HYPR_COMMIT}
git -C ~/Hyprland submodule update --init --recursive`}
      </Command>
    </Step>

    <Step index={3} title="Apply the patches and build">
      <p>
        From a Lennette checkout, run the build script. It applies{" "}
        <Code>private-overlay.patch</Code> and <Code>plugin-compat.patch</Code>,
        then compiles <Code>Hyprland</Code> and <Code>hyprctl</Code> into
        Lennette&apos;s <Code>target</Code> directory.
      </p>
      <Command label="from the lennette checkout">
        {`packaging/hyprland/build.sh ~/Hyprland\n# optional: BUILD_TYPE=Debug BUILD_JOBS=4 packaging/hyprland/build.sh ~/Hyprland`}
      </Command>
      <p>
        This does not replace your running compositor. The built binary is at{" "}
        <Code>target/hyprland-private/Hyprland</Code>.
      </p>
    </Step>

    <Step index={4} title="Load the compositor and the rules">
      <p>
        Point Hyprland at the built binary, then load the window rules that mark
        the overlay as protected. Add the rule file to your{" "}
        <Code>hyprland.lua</Code>.
      </p>
      <Command label="hyprland.lua">{RULE_LOAD}</Command>
      <p>
        An unpatched compositor rejects this file. That is deliberate: a normal
        float-and-pin rule gives no capture exclusion, so the launcher refuses
        to start rather than pretending it is protected.
      </p>
    </Step>

    <Step index={5} title="Log into the patched session">
      <p>
        Log out and start a session running the built binary. From a TTY you can
        run it directly, or point your display manager at that path.
      </p>
      <Command label="from a TTY">{TTY_CMD}</Command>
      <p>
        Once inside, confirm the rule is accepted before launching the app. Use
        the same launcher the NixOS module installs.
      </p>
      <CommandList
        commands={[
          "packaging/hyprland/launch.sh --check",
          "packaging/hyprland/launch.sh",
        ]}
      />
      <p>
        Set <Code>LENNETTE_BINARY</Code> if Lennette is not at{" "}
        <Code>~/.local/bin/lennette</Code>.
      </p>
    </Step>

    <Step index={6} title="Install the app itself">
      <p>Same as on any other system: build it from a checkout.</p>
      <Command label="from a checkout">{INSTALL_CMD}</Command>
    </Step>

    <div className="glass rounded-3xl p-6 sm:p-8">
      <div className="relative z-10">
        <p className="text-sm font-medium text-sky-deep">No shortcuts here</p>
        <p className="mt-2 max-w-2xl leading-relaxed text-ink-soft">
          On NixOS the module owns the compositor package, the launcher and the
          bindings. On other distributions you maintain all three: rebuild the
          compositor after every Hyprland or plugin change, keep the rule file
          loaded, and launch through <Code>launch.sh</Code>.
        </p>
      </div>
    </div>
  </div>
);

const TutorialPage = () => (
  <PageShell
    titleIcon={["Private on", "Hyprland."]}
    intro="Hyprland has no built-in way to hide one window from screen capture, so Lennette ships a patched compositor that does. Pick your setup below; the bindings and the protection boundary are the same either way."
  >
    <div className="glass mb-16 rounded-3xl p-6 sm:p-8">
      <div className="relative z-10">
        <p className="text-sm font-medium text-sky-deep">Before you start</p>
        <ul className="mt-3 flex flex-col gap-2 text-ink-soft">
          <li>Supported system: x86_64 Linux running Hyprland.</li>
          <li>
            This is a compositor patch, so it cannot be loaded into an existing
            Hyprland session. You build a new compositor and log into it.
          </li>
          <li>Your Hyprland configuration must be Lua-based, not Hyprlang.</li>
          <li>
            Pick the method that matches your system. NixOS automates it; other
            distributions build the compositor by hand.
          </li>
        </ul>
      </div>
    </div>

    <Tabs
      tabs={[
        { id: "nixos", label: "NixOS" },
        { id: "linux", label: "Other Linux" },
      ]}
    >
      {(active) => (active === "nixos" ? <NixosSteps /> : <DistroSteps />)}
    </Tabs>

    <div className="mt-16 flex flex-col gap-12">
      <Step index={7} title="Learn the bindings">
        <p>
          These come from the Home Manager module on NixOS. On other
          distributions, add the same keys to your own Hyprland config and point
          them at <Code>lennette</Code> with the matching flag.
        </p>
        <div className="glass overflow-hidden rounded-2xl">
          <div className="relative z-10 divide-y divide-ink/10">
            {bindings.map((binding) => (
              <div
                key={binding.keys}
                className="flex flex-col gap-3 px-5 py-4 sm:flex-row sm:items-center sm:justify-between"
              >
                <span className="flex flex-wrap items-center gap-1.5">
                  {binding.keys.split("+").map((key, index) => (
                    <span key={key} className="flex items-center gap-1.5">
                      {index > 0 ? (
                        <span
                          aria-hidden="true"
                          className="text-xs text-ink-soft/60"
                        >
                          +
                        </span>
                      ) : null}
                      <kbd className="kbd-glass">{key}</kbd>
                    </span>
                  ))}
                </span>
                <span className="text-sm text-ink-soft">{binding.action}</span>
              </div>
            ))}
          </div>
        </div>
      </Step>

      <Step index={8} title="If something breaks">
        <p>
          <span className="text-ink">
            unknown field &apos;private_overlay&apos;
          </span>{" "}
          means the new config is loaded but an old compositor is still running.
          Log out and back in. Until then the launcher will refuse to start.
        </p>
        <p>
          Plugins only work if they are rebuilt against this compositor&apos;s
          patched headers and listed by exact library path in{" "}
          <Code>programs.lennettePrivateOverlay.allowedPluginPaths</Code>. The
          launcher rejects anything outside that list.
        </p>
      </Step>
    </div>

    <section className="glass mt-16 rounded-3xl p-6 sm:p-8">
      <div className="relative z-10">
        <h2 className="font-display text-2xl text-ink">
          What this does not protect
        </h2>
        <p className="mt-3 max-w-2xl leading-relaxed text-ink-soft">
          Be precise about the guarantee. The patched compositor hides protected
          windows from its own window and monitor capture paths. It does not
          cover window metadata, notifications, input-method windows, clipboard
          or drag surfaces, or a copy of Lennette shown by another application.
          Raw GPU/KMS capture, cameras and capture cards, and other compositors
          are outside the boundary too.
        </p>
        <p className="mt-3 max-w-2xl leading-relaxed text-ink-soft">
          Before relying on it in a real call, run a private test share from a
          second device in each meeting app and share mode you plan to use.
        </p>
      </div>
    </section>

    <div className="mt-12 flex flex-wrap items-center gap-4">
      <a
        href={`${GITHUB_URL}/tree/main/packaging/hyprland`}
        target="_blank"
        rel="noreferrer"
        className="glass btn-liquid glass-pill glass-blue text-on-sky"
      >
        <span>Read the full compositor docs</span>
      </a>
      <a
        href={RELEASES_URL}
        target="_blank"
        rel="noreferrer"
        className="text-sm font-medium text-sky-deep transition-colors hover:text-ink"
      >
        Skip the patch and just download the app →
      </a>
    </div>
  </PageShell>
);

export const Route = createFileRoute("/tutorial")({
  head: () => ({
    meta: [
      { title: "Hyprland setup - Lennette" },
      {
        name: "description",
        content:
          "Step by step guide to running Lennette as a private overlay on Hyprland, on NixOS or any other Linux distribution.",
      },
    ],
  }),
  component: TutorialPage,
});
