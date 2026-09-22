# Architectural decision record (ADR) 001: Behavioural storage ports

## Status

Accepted architectural direction, 2026-09-19: the supplied conversation selects
RouchDB behind Mornington-owned behavioural ports. Production readiness and the
specific adapter implementation remain subject to the design's storage gate.

## Date

2026-09-19.

## Context and problem statement

The original proposal requires CouchDB and JavaScript map/reduce. Subsequent
exploration considers RouchDB first as a test fixture, then as Mornington's
initial production engine. The final requirement is richer than generic
create/read/update/delete operations: the adapter implements persistence,
conflicts, projections, ordering, pagination, and incremental observation.

The repository currently contains generated Rust stubs and engineering guides.
Inspection of `src/`, `tests/`, and the documentation index found no existing
storage port or projection abstraction to reuse.

## Decision drivers

- Exercise the same application adapter and Rust projections in memory-backed
  tests and a durable local installation.
- Keep the first installation to one application process without a database
  service, its credentials, or its deployment lifecycle.
- Preserve application semantics if measured requirements justify another
  engine; do not promise database protocol equivalence.

## Options considered

Table 1. Storage alternatives against the present scope.

| Option                                       | Consequence                                                                                                                     |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| CouchDB initially, RouchDB only for fixtures | Keeps the original server and JavaScript views, but fixture tests do not execute the production projection implementation.      |
| Generic repository or database REST proxy    | Exposes document operations without specifying the useful conversation behaviour; backend assumptions can leak into callers.    |
| RouchDB behind behavioural ports             | Shares adapter code across memory and redb while defining ordering, recovery, and authorization boundaries in Mornington terms. |
| Clustered alternative initially              | Adds integration and operational work before a clustered workload has been established.                                         |

## Decision outcome

Select RouchDB with redb for initial production storage and its memory backend
for fast component tests. Implement conversation, projection, authority,
session/token, changes, and export capabilities as narrow application ports. The
[technical design](mornington-design.md#4-behavioural-storage-contracts) owns
their contracts and permitted composition.

Only the adapter and composition root may depend on RouchDB types. Application
services own authorization and domain policy; the adapter owns storage-derived
behaviour and converts failures into semantic errors. A future adapter must
pass the same behavioural suite. Public cursors and logical exports are owned
by Mornington, with explicit resynchronization on engine migration.

## Known risks and limitations

RouchDB 0.4.0's `ViewEngine` keeps emitted rows and its checkpoint in memory.
Its name does not establish durable view indexes. The initial design therefore
rebuilds disposable projections and tests recovery explicitly. Source
inspection is not a durability, performance, or production-maturity
certification.

Memory tests cannot establish redb crash recovery, file locking, disk-full
behaviour, or restart cost. Both modes are mandatory in the contract suite;
durable failure tests are an additional release gate. Failure of that gate
requires an explicit design revision, not a silent return to CouchDB or an
unbounded new database implementation.

## Migration and review triggers

Use schema-versioned logical export/import to move records into a replacement
adapter. Preserve post/actor IDs and authority history; invalidate sessions,
credentials, and cursors. Reconsider the engine when corpus size, recovery
time, resource use, or multi-process requirements exceed the measured envelope.
Changing engines does not by itself make authorization or quotas distributed.
