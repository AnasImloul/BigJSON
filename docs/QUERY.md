# jsq query language

`jsq` runs **one SQL-shaped query** over a JSON file in a single streaming pass and
prints **one JSON value per line (NDJSON)** to stdout, so its output composes with
`jq`, `head`, and `wc`.

This is the complete language reference. For the CLI itself (flags, stdin, params)
run `jsq --help`.

## A first query

```sh
jsq orders.json 'from .orders[] as o where o.status == "paid" select { id: o.id, total: o.total }'
```

```json
{"id": 1, "total": 120}
{"id": 3, "total": 200}
```

The smallest useful query is just `from .path[] as x` — it emits each element of the
array at `.path`. Always **single-quote the query** so your shell doesn't expand
`[]`, `*`, or `{}`.

## Clause pipeline

Clauses must appear in this order. `from` is mandatory; everything else is optional.

```
(fields NAME = { f1, f2, ... })*      -- compile-time field-set macros
from PATH as ALIAS                    -- REQUIRED; PATH is implicitly iterated
([inner|left] join PATH as ALIAS on L == R)*
(unnest EXPR as ALIAS)*               -- array fan-out, one row per element
(where PREDICATE)?
(let NAME = EXPR (, NAME = EXPR)*)?    -- alias bindings substituted into aggregate
distinct?
(aggregate { NAME: REDUCER [where P] [?? D], ... } [by KEY[, KEY] | by rollup(...)]
  | collect by KEY)?
(having PREDICATE)?                   -- filter reduced rows; refs output by .name
(select { NAME: EXPR, ... })?         -- projection / reshape
(order by EXPR [asc|desc] (, EXPR [asc|desc])*)?
(limit N)?
```

## Paths

A path has a **root** then **segments**.

Roots:
- a leading `.` — the document root (in `from`/`join`) or the current row (elsewhere).
- a bare identifier — a `from`/`join`/`unnest` alias (`o`, `o.field`).

Segments:
- `.field` — object field.
- `["key"]` — object field by quoted key (use for keys with spaces or dots).
- `[N]` — array index (e.g. `[0]`).
- `[]` — **iterate**: emit each immediate child. This is the only iteration form
  (no `.[]`, no `*`).
- `.**` — recursive descent (this node and all descendants).
- `.{a, b, c}` — field-set; **only** valid as the left-hand side of a comparison.

Examples: `.users[]`, `o.address.city`, `.data["weird key"][0]`, `.tree.**`.

## Expressions & operators

- Comparisons: `==  !=  <  <=  >  >=`
- String / pattern: `contains`, `starts_with`, `ends_with`, `matches` (regex).
  E.g. `o.title contains "json"`, `c.name starts_with "A"`.
- Membership: `EXPR in [a, b, c]`, `EXPR not in [...]`.
- Existence: `EXPR exists` (postfix).
- Type test: `EXPR is TYPE` / `EXPR is not TYPE`, where TYPE is one of
  `string | number | bool | null | array | object`.
- Boolean: `and`, `or`, `not (...)`. A **comma inside `where` means `and`**.
- Arithmetic: `+ - * /`, unary minus, parentheses.
- Null-coalescing default: `EXPR ?? FALLBACK`.
- Conditional: `if(COND, THEN, ELSE)` — uses jq truthiness, so only `null` and
  `false` are falsy (`0` and `""` are truthy).
- Parameters: `$name` (bind with `--param name=...`).
- Literals: numbers, `"strings"`, `true`, `false`, `null`, arrays `[1, 2]`,
  objects `{ k: v }`.

### Scalar functions

Each argument collapses to its first value; non-matching input types yield `null`.

`round(x[, places])`, `length(x)` (string chars / array length / object keys; `0`
for null), `lower(s)`, `upper(s)`, `trim(s)`, `substr(s, start, len)`,
`replace(s, from, to)`, `abs(n)`, `floor(n)`, `ceil(n)`, `sqrt(n)`,
`pow(base, exp)`, `mod(a, b)`.

## Aggregation

Reducers live **only** inside an `aggregate { ... }` block, and they're always
function calls. There are five: `count()` (rows in the group) plus `sum(EXPR)`,
`avg(EXPR)`, `min(EXPR)`, and `max(EXPR)`, each folding EXPR over the group.

Each block item is `NAME: OUTPUT [where PRED] [?? DEFAULT]`, where OUTPUT may be
arithmetic over reducer calls. An item-level `where` filters which rows feed *that*
item's reducers:

```sh
jsq orders.json 'from .orders[] as o
aggregate {
  revenue: sum(o.total),
  paid:    sum(o.total) where o.status == "paid" ?? 0
} by o.region'
```

```json
{"region": "EU", "revenue": 200, "paid": 120}
{"region": "US", "revenue": 200, "paid": 200}
```

### Output shape

The presence of `by` changes what comes out, so it's worth being precise about it.

- **With `by KEY`**: emits one **object per group**, `{ <key field(s)>, <metric names…> }`.
  A following `having`/`select` can reference the metric names by path (`.revenue`,
  `.paid`). This is the usual case.
- **Without `by`**: reduces the whole stream and emits the **value(s) directly** — a
  single metric prints one bare scalar:

  ```sh
  jsq orders.json 'from .orders[] as o aggregate { n: count() }'   # → 3
  ```

  Several metrics print one bare scalar per item, in order. The names are *not*
  emitted as keys and are *not* visible to a downstream `select`. If you want a
  labeled result, group with `by` — e.g. `by true` for a single global bucket.

### Grouping

- `aggregate { ... } by KEY` — one row per distinct KEY.
- `... by K1, K2` — composite key (one row per distinct tuple).
- `... by rollup(K1, K2)` — hierarchical subtotals plus a grand total; rolled-up
  trailing keys render as `null`.

### let — reusable reducer arithmetic

`let` binds expressions that are substituted into the aggregate block. Group with
`by` to get labeled rows:

```sh
jsq stores.json 'from .stores[] as s
let actual = sum(s.actual), target = sum(s.target)
aggregate { pct: (actual - target) / target * 100 ?? 0, delta: actual - target } by s.region'
```

### collect by — gather instead of reduce

`collect by KEY` is the non-reducing sibling of `aggregate … by`: it gathers all
rows per key into a member list instead of folding them.

### having — filter reduced rows

`having PRED` filters grouped output, referencing output fields by identity path:

```sh
jsq orders.json 'from .orders[] as o aggregate { n: count() } by o.region having .n > 1'
```

```json
{"region": "EU", "n": 2}
```

## Joins & unnest

`join ... on L == R` is an inner join; `left join` keeps unmatched left rows with the
joined alias bound to `null`. You don't set anything up — the required indexes are
built for you.

`unnest EXPR as ALIAS` flattens an array field into one row per element. If EXPR is
missing, empty, or not an array, the row is dropped.

```sh
jsq orders.json 'from .orders[] as o
join .customers[] as c on c.id == o.customer_id
unnest o.items as it
select { customer: c.name, sku: it.sku, qty: it.qty }'
```

```json
{"customer": "Acme", "sku": "A", "qty": 2}
{"customer": "Acme", "sku": "B", "qty": 1}
{"customer": "Acme", "sku": "A", "qty": 1}
{"customer": "Globex", "sku": "C", "qty": 5}
```

## Subqueries

A parenthesised full query is a correlated subquery usable in expression position; it
may reference the outer query's aliases. It composes with `exists`, `in`, comparisons,
and scalar `select` (first emission wins):

```sh
jsq data.json 'from .users[] as u
where ( from .orders[] as o where o.user_id == u.id ) exists'
```

## Field-set macros

`fields NAME = { a, b, c }` defines a reusable field set before `from`. A field-set
is only legal as the left-hand side of a comparison (`.{a, b} == ...`).

## Translating from other languages

Map source constructs to clauses (think SQL/pandas, not loops):

| Imperative / functional               | jsq clause                |
|----------------------------------------|---------------------------|
| `for x in arr` / iterate a collection  | `from .arr[] as x`        |
| `filter(pred)` / `if cond: keep`       | `where PRED`              |
| `map(f)` / build a new dict per item   | `select { ... }`          |
| `flatMap` / nested loop over `x.items` | `unnest x.items as it`    |
| `groupBy(k)` + reduce                  | `aggregate { ... } by k`  |
| `groupBy(k)` gather members            | `collect by k`            |
| join across two collections            | `join ... on a == b`      |
| `set()` / `DISTINCT`                   | `distinct`                |
| `sorted(key, reverse)`                 | `order by EXPR [desc]`    |
| `[:N]` / `LIMIT`                       | `limit N`                 |
| `x or default` / coalesce              | `EXPR ?? DEFAULT`         |
| ternary `cond ? a : b`                 | `if(COND, a, b)`          |
| `x in (a, b, c)`                       | `x in [a, b, c]`          |
| post-aggregate filter (`HAVING`)       | `having PRED`             |

Python:

```python
[ {"name": u["name"], "spend": u["spend"]}
  for u in data["users"]
  if u["active"] and u["spend"] > 100 ]
```

```sh
jsq data.json 'from .users[] as u
where u.active == true and u.spend > 100
select { name: u.name, spend: u.spend }'
```

JS group-and-sum + sort + top-5:

```js
orders.filter(o => o.status === "paid")
      .reduce(byRegionSum, {})  // sum total per region, then sort desc, take 5
```

```sh
jsq orders.json 'from .orders[] as o
where o.status == "paid"
aggregate { revenue: sum(o.total) } by o.region
order by .revenue desc
limit 5'
```

## Common pitfalls

- `order by` needs an explicit path: `order by .weight` (or `alias.field`), **not**
  `order by weight`.
- Reducers only exist inside `aggregate { ... }`; there is no bare `count` /
  `sum X by Y` shorthand.
- `[]` is the only iteration form (no `.[]`, no `*`).
- Compare booleans explicitly when needed (`u.active == true`), or rely on truthiness
  (`where u.active`).
- Field-set `.{a, b}` is only legal as the left side of a comparison.

## Validating a query

`jsq --explain <FILE> '<QUERY>'` parses and lowers the query without touching data,
which lets you check syntax against a large file cheaply. A parse error prints
`parse error at position N: ...`.
