# Linux standalone sender

The DEB contains only AxiAccess's native Rust sender, its user-session startup
manager, service unit, status launcher and documentation. It contains no Python,
Node, IDE, provider CLI or AxiOwl daemon. The desktop's AT-SPI accessibility bus
and normal system C/C++ runtime libraries remain explicit OS prerequisites.

Install the authenticated package with `sudo dpkg -i AxiAccess_VERSION_amd64.deb`.
Installation does not start observation as root or change another user's session.
From the selected user's graphical desktop, run `axiaccess-session start`.
`axiaccess-session enable` enables startup for that user only; `disable` removes
the owned entry and stops the owned user unit. `status` reports the actual unit
state. On a non-systemd desktop, the autostart entry launches the sender in the
graphical environment; foreground shutdown remains controlled by that process.

The sender uses its own private configuration/identity namespace. Configure client
pairing and trust using `axiaccess --help` and the sender's management commands.
Remote clients connect with authenticated TLS or an explicit SSH tunnel. No
unauthenticated public observation port is installed. No click/type/action API
is part of AxiAccess.

Upgrade and same-version reinstall replace owned program files only. The package
does not delete pairing/configuration, provider credentials, chats, signing files,
or any user's application documents. Uninstall removes package-owned files;
user-owned configuration/pairing data is deliberately retained. An owned active
systemd user sender is stopped before replacing its image and restarted after an
upgrade or aborted upgrade; an inactive sender is not started. Removal disables
its owned user integration as that user. No root maintainer script enumerates or
kills arbitrary processes. A non-systemd foreground sender must be stopped by its
owner before upgrade/removal; it is never silently relabelled as a new image.
An offline user without a running session bus retains their user-owned startup
entry on uninstall; disable startup as that user before removal. The installer
does not walk offline users' home directories or delete their state as root.

Build from literal `main` using the one canonical command:

```sh
python3 scripts/release-linux.py
```

Build receipts record source, target, Cargo lock, toolchain, recipe, native ELF
dependencies, installed logical bytes and artifact hashes. Release signing uses
the existing development-node signer without copying private keys into source or
packages. This README describes the implementation contract; actual installation
and remote/VM acceptance require their separate receipts.
