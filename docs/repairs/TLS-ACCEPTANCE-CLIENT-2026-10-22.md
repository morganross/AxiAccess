# TLS acceptance client contract correction

Date: October 22, 2026, America/Los_Angeles.

Scope: operator acceptance harness only, not a product-code repair.

## Documents read before the correction

1. `docs/SENDER-PROTOCOL.md` in AxiAccess: the `windows` request uses `maxWindows`, not `limit`, and rejects unknown fields.
2. AxiAccess `README.md`: historical public observation modes; its legacy API names cannot be assumed to match the new authenticated protocol.
3. IDE `ide-ipc/README.md`: request identity, typed failure and bounded framing must remain authoritative; an invalid fixture is not a successful operation.
4. IDE `docs/MCP-FRAMEWORK-CONFIGURATION.md`: configuration checks and real server observations must be reported separately and preserve external credentials.
5. IDE `docs/IDE-ORCHESTRATION.md`: scoped transport, ownership and native results must not be replaced by synthetic pass claims.

These five distinct documents were read in full immediately before modifying the harness. The final implementation plan was also already read and supplies the observation-only and preservation scope.

## Evidenced cause and change

The first actual TLS exercise correctly rejected the harness's `windows` request because the harness supplied the unrecognized field `limit`. The product returned `invalid_arguments`, with the explicit message that `maxWindows` is expected. This is a client-fixture mistake, not evidence that valid window reads failed.

Change: the task-owned `output/axiaccess-implementation-20261022/tls_native_acceptance.py` now supplies `maxWindows: 250` from the documented protocol. No permissive product parsing or fallback was introduced.

## Boundaries and verification

The exercise launches only its exact owned sender process and uses an isolated AxiAccess configuration under the current Windows user's LocalAppData, outside Git and OneDrive. Pairing tokens/private keys are not printed or included in evidence. Provider chats, credentials and the daemon are not touched. No click, typing, UI action or inference is performed.

The initial run already demonstrated that the actual TLS sender rejected an invalid token and browser Origin and accepted authenticated capabilities before reaching the invalid inventory fixture. The corrected full run's actual receipt is recorded separately; this document does not predeclare that rerun successful. Linux/Mac behavior is not established by this Windows exercise.
