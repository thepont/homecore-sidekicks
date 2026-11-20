# Currency inference data

`src/currency_data.rs` is a generated subset of geographic IANA timezone
territories and CLDR legal-tender currency intervals. It adds no runtime
dependencies, network requests, locale settings or data-file loading.

The lookup uses the site's exact timezone name in IANA `zone.tab`. That table
associates each geographic zone with one territory. It deliberately does not
use `zone1970.tab`, which groups territories that share clock rules, or infer
territory from UTC offset. Aliases absent from `zone.tab` remain unsupported.

For the site's local date, a currency is returned only when the territory has
exactly one active legal-tender code. Multiple tenders, transition overlaps,
dates outside known intervals, Antarctica and non-geographic zones produce no inference.
CLDR date-only `from` and `to` bounds include the entire named local day.
Non-tender financial instruments are omitted. The supplied tariff currency
takes precedence; a territory's usual currency is an inference about a tariff,
not proof of its billing currency.

The generator accepts the pinned source layout after checksum verification.
It validates codes, dates, unique timezone names and bounded input sizes.
Explicit transition timezones are accepted only for territories with a single
matching geographic zone. Intraday transitions are rejected, so updating the
source cannot silently discard finer time semantics.

## Sources and licences

| Source | Pinned revision | SHA-256 |
| --- | --- | --- |
| [IANA tz 2026b, zone.tab](https://github.com/eggert/tz/blob/48c25a1ba86cb602990c0573aba7795417931bb4/zone.tab) | `48c25a1ba86cb602990c0573aba7795417931bb4` | `4d8e389e5f4b0ec0466d5b14f42e5dfb0308c4376165fcf478339afd9ddcb00c` |
| [CLDR 49, supplementalData.xml](https://github.com/unicode-org/cldr/blob/96e54b273d2da4ec6fbad350ba19ff23fa24a588/common/supplemental/supplementalData.xml) | `96e54b273d2da4ec6fbad350ba19ff23fa24a588` | `745150ec1408d50e1a272b65654a5dd645f3e60f5cc2898b576f725b28538d1a` |

IANA `zone.tab` is public domain, as stated in its header. The CLDR-derived
data is covered by the accompanying [Unicode License V3](LICENSE-Unicode).
The licence file SHA-256 is
`65eb33419e22a297550a811b5b535f5e532f9a7e0114b516c0ea18afd53c3804`.

Semantics follow CLDR's [supplemental currency data](https://www.unicode.org/reports/tr35/tr35-79/tr35-numbers.html#Supplemental_Currency_Data)
and [date ranges](https://www.unicode.org/reports/tr35/tr35-79/tr35.html#Date_Ranges).
CLDR orders overlapping tenders by preference; this adapter conservatively
leaves those intervals unspecified instead of selecting the preferred one.

## Regeneration

Run from the repository root. Checksum verification is mandatory before the
offline extractor reads the downloaded inputs. Neither downloaded source is
used at runtime. `cmp` should exit successfully for the pinned versions.

```sh
set -eu
currency_data_dir=$(mktemp -d)
curl -fsSL https://raw.githubusercontent.com/eggert/tz/48c25a1ba86cb602990c0573aba7795417931bb4/zone.tab -o "$currency_data_dir/zone.tab"
curl -fsSL https://raw.githubusercontent.com/unicode-org/cldr/96e54b273d2da4ec6fbad350ba19ff23fa24a588/common/supplemental/supplementalData.xml -o "$currency_data_dir/supplementalData.xml"
(
  cd "$currency_data_dir"
  sha256sum -c <<'CHECKSUMS'
4d8e389e5f4b0ec0466d5b14f42e5dfb0308c4376165fcf478339afd9ddcb00c  zone.tab
745150ec1408d50e1a272b65654a5dd645f3e60f5cc2898b576f725b28538d1a  supplementalData.xml
CHECKSUMS
)
rustc --edition 2021 services/teslemetry/data/currency/generate.rs -o "$currency_data_dir/generate"
"$currency_data_dir/generate" "$currency_data_dir/zone.tab" "$currency_data_dir/supplementalData.xml" "$currency_data_dir/currency_data.rs"
cmp services/teslemetry/src/currency_data.rs "$currency_data_dir/currency_data.rs"
```

To update, review the upstream changes and licence, pin the new revisions and
checksums, regenerate the table and run the currency and tariff tests. Currency
changes require refreshing this snapshot; a future change unknown to this
release cannot be inferred automatically.
