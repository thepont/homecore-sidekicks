# Electricity prices

Open the Powerwall device in homeCore and look for **Buy price** and **Feed-in
price**. These are the prices in the Powerwall's configured tariff. They describe
one kWh bought from the grid or sent back to it. The adapter reads the plan
through Teslemetry and leaves the plan and battery controls alone.

The checked site's plan has a free window between 11:00 and 15:00 Melbourne
time. A buy price of zero during that window is a real reading. A missing price
means something else: the adapter cannot supply it. Keep those two cases
separate when displaying the values.

Amounts come through in currency units per kWh. If a tariff supplies `AUD`, a
value of `0.25` means 25 Australian cents per kWh and its schema unit is
`AUD/kWh`. A supplied currency code always wins.

The checked tariff leaves the code out. The adapter uses Tesla's installation
timezone and [IANA and Unicode CLDR data](../services/teslemetry/data/currency/README.md)
to look up `Australia/Melbourne → AU → AUD`. The Powerwall card shows `AUD/kWh`
and keeps the original amount. The device attributes record how the adapter
determined the code. For an unmapped zone such as `UTC` or a region with several
active currencies, the code remains `unspecified` and the unit is `currency/kWh`.

## Fields available to cards

- `tariff_buy_price_per_kwh` and `tariff_sell_price_per_kwh` hold the prices.
  Zero and negative amounts are kept. An absent sell rate stays absent.
- `tariff_buy_period` and `tariff_sell_period` identify the selected periods.
  A flat rate uses `ALL`.
- `tariff_currency`, plus `tariff_name` and `tariff_provider` when supplied,
  describe the plan.
- `tariff_currency_source` is `tariff` for a supplied code, `site_timezone` for
  an inferred code, or `unspecified` when neither is available.
- `tariff_currency_region` gives the inferred territory, such as `AU`. It is
  absent for a supplied or unknown currency.
- `tariff_available` is true when a current buy price can be resolved.
- `tariff_updated_at` dates the configuration. A fixed plan can keep the same
  timestamp for months.

All fields are read-only. Cards must check both the device's overall
availability and `tariff_available` before showing a price as current. A failed
live Powerwall poll marks the device offline but leaves its last MQTT state
retained, so a retained number on its own proves very little.

## Schedule and connection handling

The adapter reads `tariff_content_v2` and `site_info` on its existing event
stream. Site information supplies the installation timezone. Each successful
live poll checks the schedule again in that timezone, including daylight
saving. A scheduled price change appears with the next successful poll; the
default delay between polls is 60 seconds.

Buy and sell can have different schedules. An overnight period belongs to its
starting day and season. Period starts are included and ends are excluded.
Flat `ALL` prices work too. Fallback rates replace a missing specific rate;
they are not added to it.

A null tariff withdraws it. Invalid replacements, missing timezone, gaps and
overlapping matches produce no guessed prices. Omitted weekday and clock
scalars mean zero, matching the checked payload; nonempty seasons need valid
explicit date bounds. On disconnect, the adapter clears both snapshots and
waits for them again. The next successful live publication removes unavailable
price fields from state and schema.

The plan stored in Tesla must be kept up to date if it is to agree with the
retailer's prices. The adapter cannot establish that agreement itself.

## Tests

From `services/teslemetry`, run:

```sh
cargo test --locked --test tariff_test --test tariff_stream
```

With Docker running, check MQTT publication and price removal:

```sh
cargo test --locked --test teslemetry_adapter given_tariff_stream -- --ignored --test-threads=1
```

These tests use local fixtures. The [deployment guide](../deploy/README.md)
covers the service and its private configuration.
