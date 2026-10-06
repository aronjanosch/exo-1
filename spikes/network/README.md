# Spike 4 — Client-Autorität (Wegwerf-Prototyp)

Godot **4.7.2**, Forward+/Vulkan und Jolt. Jedes Spiel simuliert sein eigenes
Schiff. Ein Host leitet die Zustände weiter; fremde Schiffe werden über
Hermite/Slerp dargestellt. Die kleine, flache Testfläche verwendet die echten
Schiffs- und Laufcontroller aus Spike 3, ohne das teure Planetenterrain.

## LAN-Abschlusstest: zwei Rechner

Auf beiden Rechnern denselben Quellstand mit Godot 4.7.2 verwenden. Im
Projektordner starten. `HOST_IP` durch die LAN-IPv4-Adresse des Hosts ersetzen.
Der Host lauscht standardmäßig auf allen IPv4-Schnittstellen. Benötigt wird
**UDP-Port 17440** am Host; `--port=...` ändert ihn auf beiden Seiten.
Die Host-Firewall muss diesen UDP-Port aus dem LAN zulassen. WLAN mit
Client-Isolation verhindert die Verbindung. Im gemeinsamen LAN braucht es
keine Router-Portweiterleitung.

Host, Rechner A:

```sh
godot --path . res://spikes/network/main.tscn -- --host --slot=1 --port=17440
```

Client, Rechner B:

```sh
godot --path . res://spikes/network/main.tscn -- --connect=HOST_IP --slot=2 --port=17440
```

Erwartet: Auf beiden Seiten steht `remotes 1`. Beide fliegen, drehen, starten
und landen abwechselnd. `Tab` verfolgt das andere Schiff. Das eigene Schiff
muss sofort reagieren; das andere wird absichtlich etwa 150 ms später
gezeigt, zuzüglich der echten Netzwerklaufzeit. Danach auf einer Seite `O`
drücken: Beim anderen darf dadurch kein Positionssprung entstehen.

Für die Fremdkabine: Der Host fliegt; der Client drückt `B` und läuft mit
WASD in dessen Schiff. Auf der Host-Seite muss die grüne Spielfigur relativ
zur Kabine stehen bzw. laufen. `O` auf dem Client darf sie nicht herauswerfen.
`F` setzt den Client wieder auf den Sitz seines eigenen Schiffs. `B` und
dieser Sitzwechsel sind ausdrücklich Test-Platzierungen, kein Boarding-Design.

Client beenden und mit derselben Slot-Nummer erneut verbinden. Keine Fehler
und kein dauerhaftes Geisterschiff erwartet; alte Anzeigen verschwinden nach
spätestens zwei Sekunden. **Dieser Test auf zwei physischen Rechnern ist
manuell offen.** Lokale ENet-Verbindungen und dieselbe LAN-Bindung sind
automatisiert geprüft.

## Steuerung und weitere Checks

- WASD: Schiff vorwärts/rückwärts/seitwärts; Space/Ctrl: hoch/runter.
- Klick fängt die Maus, Maus dreht; Esc gibt sie frei. Q/E: rollen; H: Assist.
- `Tab`: fremdes Schiff verfolgen / eigene Kamera.
- `F`: in der eigenen Kabine laufen / eigener Sitz. `B`: fremde Kabine.
- `O`: Test-Shift um (10 km, 10 km, −10 km).
- Zwei Schiffe berühren lassen: unterschiedliche lokale Reaktionen sind ein
  dokumentiertes Problem der getrennten Autoritäten, kein gelöster Kontakt.

Für eine gut sichtbare automatische Bewegung dem Host `--bot` geben. Er
startet (0–4 s), fliegt (4–10 s), dreht (10–16 s), landet (16–24 s), hält.
Bot-Läufe fangen die Maus niemals. Für Agenten gilt der lokale Wrapper:
`GODOT_AGENT_WORKSPACE=7 godot-agent` statt `godot`.

Störungstest: Auf **beiden** Seiten diese Optionen ergänzen:

```text
--rate=30 --buffer=150 --delay=150 --jitter=20 --loss=10
```

`delay` ist künstliche **Einweg-Laufzeit in ms**, `jitter` ±ms, `loss` Prozent
unabhängig verlorener Snapshots. `buffer` ist **zusätzlich** zur künstlichen
Laufzeit. Hier wird das andere Schiff also etwa 300 ms später gezeigt,
zuzüglich echter LAN-Laufzeit. Puffer-Aussetzer halten die letzte Position;
es wird nicht durch einen möglichen Zusammenstoß extrapoliert.

Weitere Optionen:

| Option | Zweck |
| --- | --- |
| `--bind=127.0.0.1` | Host nur für lokale Tests erreichbar; Standard `0.0.0.0` |
| `--connect=IP` | Client-Ziel; Standard `127.0.0.1` |
| `--slot=3` … `--slot=8` | zusätzliche Clients, eindeutige Nummern |
| `--planet=1` | Client auf zweiter Testplanetenkordinate, 200 km entfernt |
| `--shift=10` | automatischer Shift ab 10 m Entfernung; Standard 1000 m |
| `--reference` | auf beiden Seiten: Host simuliert das Client-Schiff, ohne Prediction |
| `--seconds=30` | nach 30 s Messwerte schreiben und beenden |
| `--observe` | mit Verfolgerkamera des anderen Schiffs starten |

Der Host-Autoritätsvergleich umfasst ein ferngesteuertes Client-Schiff auf
Planet 0; die Kabine ist darin nicht freigeschaltet. Das ist eine einfache
Latenz-/CPU-Referenz, kein fertiger Host-Controller mit Prediction.

## Automatisierte Prüfung

```sh
python3 spikes/network/run_suite.py
```

Ohne Zusatzabhängigkeiten: Python 3 und `godot` im PATH. Dauer rund
90 Sekunden. 96 Konfigurationen mit echten aufgezeichneten Jolt-Flugbewegungen,
Kontakt- und Kabinentests; anschließend echte ENet-Instanzen mit 2 und 8
Spielern, MCP, Host-Referenz und unterschiedlichen Planeten, LAN-Adressbindung und Wiederverbinden. Die ENet-Läufe
brauchen lokale UDP-/TCP-Sockets. Nur die Matrix braucht keine Sockets:

```sh
python3 spikes/network/run_suite.py --matrix-only
python3 spikes/network/run_suite.py --case=client-8
```

Laufdaten und Logs: `/tmp/exo-spike4-results/`. Separates `user://` unter
`/tmp/exo-spike4-data/`; es überschreibt keine Ergebnisse anderer Spikes.
Eingefrorene Messergebnisse liegen in `results/`, Einordnung im
`SPIKE-4-REPORT.md`. Feste Seeds testen unabhängigen Verlust, keine beliebigen
Verlustbursts oder WAN-Verbindungen.

## MCP für Bots

Optional das Spiel mit `--agent-port=18441` starten; der Steuerport ist
**TCP, nur localhost**, unabhängig vom LAN-Spielport. MCP-Server starten:

```sh
python3 spikes/network/mcp_bridge.py --port=18441
```

Stdio-MCP: `get_state` und `do_action`. Feste Aktionen: `pilot` (movement
`[x,y,z]`, yaw, pitch, roll), `walk_own`, `board_remote` (owner), `walk`
(movement), `shift` (offset), `observe`, `place_ship` (position, velocity).
Kein `eval`, keine Dateipfade oder Skripte. Der automatisierte Lauf prüft
MCP-Initialisierung, Werkzeugliste, Lesen, Betreten und Shift der Fremdkabine.

## Weitergabe

`dist/exo-spike4-source.zip` enthält ein kleines eigenständiges Godot-Projekt
mit dieser Szene als Startszene, ursprünglichen Controllern und Anleitung.
Entpacken, Godot 4.7.2 verwenden; die obigen Host-/Client-Kommandos gelten dort
ebenfalls. Keine fertige Binary: Auf dieser Maschine sind keine Export-Templates
installiert. Ein Quellpaket genügt gemäß Auftrag.
