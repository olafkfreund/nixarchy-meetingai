import { createFileRoute } from "@tanstack/react-router";

import { Command, CommandList } from "@/components/command";
import { GITHUB_URL, PageShell, RELEASES_URL } from "@/components/page-shell";

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

const FLAKE_LINE =
  'inputs.lennette-private.url = "github:parazeeknova/lennette?dir=packaging/hyprland";';
const INSTALL_CMD = "devenv shell -- just install";

const bindings = [
  { keys: "Ctrl+Shift+D", action: "Open the dashboard" },
  { keys: "Ctrl+Shift+Backslash", action: "Show or hide the overlay window" },
  { keys: "Ctrl+Shift+I", action: "Focus the prompt input" },
  { keys: "Ctrl+Shift+M", action: "System audio command" },
  { keys: "Ctrl+Shift+A", action: "Voice input command" },
  { keys: "Ctrl+Shift+S", action: "Screenshot command" },
];

const TutorialPage = () => (
  <PageShell
    title="Running Lennette privately on Hyprland"
    intro="Hyprland has no built-in way to hide one window from screen capture, so Lennette ships a patched compositor that does. This page walks through the install, the bindings, and what the protection does and does not cover."
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
            Nix flake with Home Manager already integrated is assumed for the
            steps below.
          </li>
        </ul>
      </div>
    </div>

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
          <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
            alice
          </code>{" "}
          with your own Home Manager user.
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
          Rebuild before activation. Replace{" "}
          <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
            my-host
          </code>{" "}
          with the host name from your own flake.
        </p>
        <CommandList
          commands={[
            "nix flake update lennette-private",
            "nixos-rebuild build --flake .#my-host",
            "sudo nixos-rebuild switch --flake .#my-host",
          ]}
        />
        <p className="text-ink">
          A switch alone does not replace the running compositor. Save your
          work, log out of Hyprland, and log back in.
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
          checkout, install the binary and make sure{" "}
          <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
            ~/.local/bin
          </code>{" "}
          is on your <code>PATH</code>.
        </p>
        <Command label="from a checkout">{INSTALL_CMD}</Command>
        <p>
          Quit any running Lennette instance before using private mode. A
          desktop entry named{" "}
          <span className="text-ink">Lennette (private overlay)</span> is added,
          and the ordinary Lennette entry is routed through the protected
          launcher.
        </p>
      </Step>

      <Step index={6} title="Learn the bindings">
        <p>
          These come from the Home Manager module, which owns the config file so
          Lennette cannot overwrite your shortcuts.
        </p>
        <div className="overflow-hidden rounded-2xl border border-ink/10">
          <table className="w-full text-left text-sm">
            <thead className="bg-ink/5 text-ink">
              <tr>
                <th className="px-4 py-3 font-medium">Keys</th>
                <th className="px-4 py-3 font-medium">Action</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-ink/10">
              {bindings.map((binding) => (
                <tr key={binding.keys}>
                  <td className="px-4 py-3 whitespace-nowrap">
                    <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
                      {binding.keys}
                    </code>
                  </td>
                  <td className="px-4 py-3 text-ink-soft">{binding.action}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Step>

      <Step index={7} title="If something breaks">
        <p>
          <span className="text-ink">
            unknown field &apos;private_overlay&apos;
          </span>{" "}
          means the new config is loaded but the old compositor is still
          running. Log out and back in. Until then the launcher will refuse to
          start.
        </p>
        <p>
          Plugins only work if they are rebuilt against this compositor&apos;s
          patched headers and listed by exact library path in{" "}
          <code className="rounded bg-white/45 px-1.5 py-0.5 text-ink">
            programs.lennettePrivateOverlay.allowedPluginPaths
          </code>
          . The launcher rejects anything outside that list.
        </p>
      </Step>
    </div>

    <section className="mt-16 rounded-3xl border border-ink/10 p-6 sm:p-8">
      <h2 className="font-display text-2xl text-ink">
        What this does not protect
      </h2>
      <p className="mt-3 max-w-2xl leading-relaxed text-ink-soft">
        Be precise about the guarantee. The patched compositor hides protected
        windows from its own window and monitor capture paths. It does not cover
        window metadata, notifications, input-method windows, clipboard or drag
        surfaces, or a copy of Lennette shown by another application. Raw
        GPU/KMS capture, cameras and capture cards, and other compositors are
        outside the boundary too.
      </p>
      <p className="mt-3 max-w-2xl leading-relaxed text-ink-soft">
        Before relying on it in a real call, run a private test share from a
        second device in each meeting app and share mode you plan to use.
      </p>
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
          "Step by step guide to running Lennette as a private overlay on Hyprland, with the patched compositor, Home Manager module and keybindings.",
      },
    ],
  }),
  component: TutorialPage,
});
