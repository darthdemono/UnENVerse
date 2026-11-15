# Password recipes

A recipe is a template that makes an ordinary password. It is configuration, not a scheme: no recipe is built in, the presets are starting points you edit, and with no recipe the generator works as before. A made password is stored like any other, so `get`, `exec`, `env`, `render`, the export formats and an older desktop all see the whole password and nothing else.

```
unv recipe add social '{prefix}-{list:food}-{random:12:base62}' --require upper+digit
unv recipe rule add provider Instagram social
unv recipe apply Instagram
```

## The template

Text outside braces is literal, and `{{` and `}}` are literal braces. A token is `{kind:arg:arg|transform:arg}`: colons separate arguments, a pipe feeds the value through a transform, quotes keep spaces and colons in an argument, and `$name` reads one of your facts. `{suffix?1}` marks a token droppable: when a site's maximum length forces a shorter password, droppable tokens go first, highest number first.

`unv recipe tokens` lists every token and transform with what it does and what it adds to the strength; the Tools pane draws its forms from the same list. The groups are Random, Lists, Entry data, Dates and counters, Your facts, Typed and linked, Transforms and Lengtheners.

Examples that all work:

| Template                                                          | What it makes                                                |
| ----------------------------------------------------------------- | ------------------------------------------------------------ |
| `{random:20:safe}`                                                | twenty random characters without look-alikes                 |
| `{prefix}{divider}{random:16:base62}`                             | a word, a divider, sixteen random characters                 |
| `{prefix}_{list:food}_{random:10:base36}`                         | a word, a food from a list, random characters                |
| `{provider}_{created:D_M_YYYY}_{random:22:base36}_{digits:3}`     | the provider, the date the entry was made, then random parts |
| `{provider\|lower}.{var:pet\|title}.{random:12:base62}{suffix?1}` | a fact of yours and a tail dropped first                     |
| `{provider}_{random:8:base36\|expand:22:base36:host}_{suffix}`    | a short random part lengthened per host                      |
| `{mask:"CvcvCvcv"}-{number:100..999}-{symbols:1}`                 | a pronounceable mask                                         |

## What the strength figure means

Only random draws add bits: a random character adds log2 of its alphabet, a list pick adds log2 of the list's size, and a date, the provider or a fact adds none. The preview shows the figure if the scheme is known, how much stays hidden if your private lists and facts stay private, what each part adds, and whether the result could be rebuilt from memory (it can if no part is random). A recipe below the floor (64 bits by default, `--floor` or the pane to change it) generates nothing. Small lists warn: 32 words is 5 bits a pick.

## Lengtheners

`expand`, `expand_words`, `expand_groups`, `site_bound`, `family_bound` and `rotate_by_counter` derive a longer value from a short one with HKDF-SHA256. They add length and no strength, so a random part of 8 characters expanded to 40 still counts 8 characters of bits. The context decides what is shared: `host` gives every host its own result, `family` gives every host of a site family (see [Sites and login plans](sites-and-login-plans.md)) one result, `counter` gives a new result by bumping a number. The construction is versioned and its results never change.

## Options

Shortest and longest length with a policy (fit, or refuse), required classes with counts, no look-alikes, forbidden or only-allowed characters, a longest run, no sequences, start with a letter, no symbol at the ends, grouping, a strength floor and a reuse check against the entry's earlier passwords. Constraints are met by drawing again, which keeps every accepted result uniform.

## Which recipe applies

An entry's own pick, then the first matching rule (provider, site family, host, type, project or tag), then the vault default, then plain random. `none` as a rule's recipe switches recipes off for a match.

## Your lists and your facts

`unv recipe list add NAME WORD...` (or `--file`) keeps a list of yours, which wins over a built-in list of the same name. `unv recipe fact set pet rex` keeps a fact for `{var:pet}`; facts are private unless `--public`, and `unv recipe fact ls` redacts private values.
