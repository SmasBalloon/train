# Contribuer à Rainhill

Merci de vouloir améliorer le jeu ! Corrections, équilibrage, nouvelles
locomotives, recherches ou époques, documentation : toute contribution est la
bienvenue, tant qu'elle respecte les quelques règles ci-dessous.

## La règle d'or : le terminal

Rainhill est et restera un jeu **dans le terminal** : c'est une condition de sa
[licence](LICENSE). Les propositions d'interface graphique, web ou mobile, ou
qui réutilisent le code ou les données dans un tel programme, ne pourront pas
être acceptées.

## Avant de commencer

- Pour un **petit correctif** (bug, faute de frappe, valeur d'équilibrage),
  ouvre directement une pull request.
- Pour une **fonctionnalité** ou un changement de règle du jeu, ouvre d'abord
  une issue pour en discuter : ça évite de travailler sur quelque chose qui ne
  serait pas retenu.

## Mettre en place le projet

Il faut une chaîne Rust stable récente (édition 2024).

```sh
git clone https://github.com/SmasBalloon/train.git
cd train
cargo run            # lance le jeu
```

Pour ne pas toucher à ta propre partie pendant les essais, utilise un dossier
de données à part :

```sh
RAINHILL_DATA_DIR=/tmp/rainhill-dev cargo run -- --new
```

## Avant d'envoyer une pull request

Le CI lance exactement ces vérifications, sous Linux, macOS et Windows :

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

Aucun avertissement de clippy n'est toléré. Toute nouvelle règle de jeu ou tout
correctif vient avec un **test** qui la vérifie.

## Repères dans le code

- **`src/sim/`** : la simulation. Elle est **pure et déterministe** : pas
  d'accès au terminal, au disque, au réseau ni à l'horloge, et pas de hasard
  hors des générateurs à graine déjà en place. Une même partie doit toujours
  donner le même résultat.
- **`src/ui/`** : l'interface, avec [ratatui](https://ratatui.rs). Elle doit
  aussi fonctionner en mode `--ascii` et en `--no-color`.
- **`data/`** : toutes les règles du jeu en TOML (locomotives, recherches,
  époques, équilibrage…). Un changement d'équilibrage se fait là, pas dans le
  code. Les données sont validées au chargement (`src/data/validate.rs`) :
  ajoute une validation pour tout nouveau champ.
- **Sauvegarde** : si tu modifies la structure de `World`, donne aux nouveaux
  champs une valeur par défaut (`#[serde(default)]`) et complète
  `World::reconcile`, pour que les parties existantes continuent de se charger.

Le jeu, ses textes et les commentaires du code sont **en français**.

## Messages de commit

Le patch note de chaque release est généré à partir des commits : merci de
suivre le format [Conventional Commits](https://www.conventionalcommits.org/fr/).

```
feat(shop): achat de plusieurs locomotives à la fois
fix: le contrôleur n'était pas payé en réserve
```

| Type       | Pour                                    | Effet sur la version |
|------------|-----------------------------------------|----------------------|
| `feat`     | une nouveauté                           | mineure              |
| `fix`      | une correction                          | correctif            |
| `perf`     | une amélioration de performance         | correctif            |
| `refactor` | une réorganisation sans changement visible | correctif         |
| `docs`, `test`, `build`, `ci`, `chore` | le reste            | correctif            |

Un `!` après le type (`feat!:`) ou une ligne `BREAKING CHANGE:` dans le corps
signale un changement cassant (par exemple, des sauvegardes qui ne se chargent
plus).

## Droits sur ta contribution

En proposant une contribution (pull request, patch ou autre), tu acceptes que :

1. **Elle est distribuée sous la licence du projet**, la
   [Rainhill Terminal License](LICENSE), comme le reste du jeu.
2. **Tu accordes à l'auteur du projet** (SmasBalloon) une licence perpétuelle,
   mondiale, gratuite et irrévocable sur ta contribution, avec le droit de la
   distribuer sous d'autres conditions, d'accorder des exceptions à la licence
   (par exemple l'autorisation écrite prévue à sa section 3) et de changer la
   licence du projet. Tu restes l'auteur de ton code et libre d'en faire ce que
   tu veux par ailleurs.
3. **Tu as le droit de la proposer** : c'est ton travail, ou tu as le droit de
   le soumettre dans ces conditions. C'est ce que certifie le
   [Developer Certificate of Origin](https://developercertificate.org/) (DCO).

Pour l'indiquer, **signe chacun de tes commits** avec l'option `-s` :

```sh
git commit -s -m "fix: le contrôleur n'était pas payé en réserve"
```

Git ajoute alors une ligne `Signed-off-by: Ton Nom <ton@email>` à la fin du
message. Un commit oublié se corrige avec `git commit --amend -s`, ou pour
toute une branche avec `git rebase --signoff master`.

## Signaler un problème

Ouvre une [issue](https://github.com/SmasBalloon/train/issues) en indiquant ton
système, ton terminal, la version (`rail --version`) et comment reproduire le
problème. Ne joins pas ta sauvegarde si elle contient des choses que tu ne veux
pas partager.
