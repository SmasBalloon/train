# Rainhill

Jeu incrémental de réseau ferroviaire **dans le terminal**, de 1825 à demain.

Tu démarres avec une ligne, une locomotive et 50 £ : la Stockton & Darlington,
à l'aube du chemin de fer. Recrute un conducteur, transporte des voyageurs,
finance la recherche, construis de nouvelles lignes à travers le monde et
traverse les époques, de la vapeur jusqu'aux trains du futur.

## Installation

Il faut une chaîne Rust récente (édition 2024).

```sh
cargo install --path .
```

Le binaire s'appelle `rail`. Sans installation, on peut aussi jouer avec
`cargo run --release`.

## Lancer une partie

```sh
rail              # interface interactive (carte du réseau)
rail --new        # recommence une partie (l'ancienne est gardée en save.json.bak)
```

| Commande        | Effet                                                        |
|-----------------|--------------------------------------------------------------|
| `rail map`      | carte du réseau en direct, trains animés                     |
| `rail board`    | tableau des départs et arrivées, façon gare                  |
| `rail fleet`    | ouvre directement la flotte                                  |
| `rail research` | ouvre directement l'arbre de recherche                       |
| `rail upgrade`  | ouvre directement les améliorations de ligne                 |
| `rail status`   | une ligne d'état (argent, trains en route, revenu) pour un prompt ou tmux |

Options d'affichage : `--ascii` (sans caractères Unicode), `--no-color`
(la variable `NO_COLOR` est aussi respectée), `--colorblind` (palette adaptée
au daltonisme).

### Jouer en arrière-plan

```sh
rail daemon start     # le réseau tourne en arrière-plan, avec notifications de bureau
rail daemon status
rail daemon stop      # dernière sauvegarde, puis rend la main
rail daemon run       # au premier plan (systemd, ou pour voir une erreur)
```

Pendant que le démon tourne, `rail` ouvre la partie en **lecture seule**.
Ajoute `--no-notify` pour couper les notifications (`notify-send` sous Linux,
`osascript` sous macOS).

### Absence

Le jeu avance aussi quand il est fermé : à la réouverture, l'absence est
rattrapée (jusqu'à 8 h réelles, à 50 % d'efficacité par défaut) et un résumé
s'affiche. Une seconde réelle vaut deux minutes de jeu.

## Commandes

| Touche                    | Action                                                    |
|---------------------------|-----------------------------------------------------------|
| `d`, Entrée               | donner le départ au train sélectionné                     |
| Espace                    | donner le départ à tous les trains prêts                  |
| Tab                       | sélectionner le train suivant                             |
| `a`                       | dépôt : acheter des locomotives (gauche/droite : quantité) |
| `v`                       | flotte : revendre, remplacer une machine                  |
| `s`                       | équipages : recruter, affecter au train sélectionné       |
| `i`                       | formations : programmer une session, inscrire des équipiers |
| `r`                       | bureau d'études : arbre de recherche                      |
| `n`                       | réseau : concessions, lignes à construire                 |
| `e`                       | époques : conditions pour passer à la suivante            |
| `u`                       | améliorations de la ligne (rails, caténaire…)             |
| `c`                       | combustibles : stocks et cours du jour                    |
| `t`                       | tarifs et horaires, satisfaction des clientèles           |
| `g`                       | services : automatiser réparations, achats, horaires      |
| `o`                       | compagnie : nationaliser, bonus permanents                |
| `m`                       | remettre la voie à neuf                                   |
| `b`                       | tableau des départs et arrivées                           |
| `f`                       | suivre le train sélectionné                               |
| `+` / `-`, molette        | zoomer, dézoomer                                          |
| Flèches, `hjkl`, glisser  | déplacer la carte                                         |
| `0`                       | recadrer la ligne courante                                |
| `1` / `2` / `3`           | vue gare, pays, continent                                 |
| `p`                       | pause                                                     |
| `?`                       | aide                                                      |
| `q`, Échap                | quitter (la partie est sauvegardée)                       |

## Mécaniques

### Équipages

Un train **ne part jamais sans conducteur**, et n'en a **jamais plus d'un**.
Ouvre les équipages (`s`) et embauche un conducteur dans l'onglet Recrutement :
il prend aussitôt un train qui n'en a pas (le train sélectionné d'abord), ou
attend en réserve si tous sont pourvus. Les trains neufs reçoivent les
conducteurs en réserve. La prime du tout premier conducteur est offerte, pour
pouvoir démarrer.

Pour changer de conducteur, sélectionne le train, puis Entrée sur un employé
dans l'onglet Employés : le titulaire retourne en réserve. Entrée une seconde
fois renvoie le conducteur en réserve.

- **Conducteur** : conduite (accélération et freinage), économie (consommation),
  douceur (usure de la voie).
- **Contrôleur** (facultatif) : accueil (confort), contrôle (recettes),
  embarquement (vitesse de montée des voyageurs).

Les compétences, notées sur cinq étoiles, progressent avec les kilomètres
parcourus. Un équipier mieux noté demande un meilleur salaire, payé même en
réserve.

### Formations

Dans la fenêtre des formations (`i`, ou `f` depuis les équipages) :

1. `n` **programme** une session : un métier, une compétence. Elle commence
   6 h de jeu plus tard et dure 8 h.
2. Entrée sur la session ouvre les **inscriptions** : jusqu'à **5 équipiers**,
   payés à l'inscription (plus le niveau est haut, plus c'est cher). On peut
   se désinscrire ou annuler (`x`) jusqu'au début : tout est remboursé.
3. À la fin, chaque inscrit gagne +10 % dans la compétence, avec **10 % de
   risque de ne rien retenir**.

Les inscrits sont absents pendant la session : un conducteur en formation
laisse son train à quai.

### Flotte

- `x` revend un train. À quai, la vente est immédiate. En route, elle est
  **programmée pour son arrivée** ; appuyer de nouveau sur `x` l'annule.
- `r` **remplace la machine** sans passer par la revente : le train garde sa
  ligne, son numéro et son équipage, et on paie le prix de la nouvelle machine
  moins la reprise de l'ancienne. Si le train roule, le remplacement se fait à
  son arrivée (payé à la commande, remboursé en cas d'annulation).

### Et aussi

Recherche, époques historiques, concessions dans plusieurs pays (écartements
différents), améliorations de voie, marché des combustibles, catégories de
voyageurs et satisfaction, événements historiques à trancher, services
d'automatisation, et nationalisation pour gagner des points de prestige
permanents.

## Sauvegarde et mods

La partie est enregistrée dans le dossier de données de l'utilisateur
(`~/.local/share/rainhill/save.json` sous Linux). La variable
`RAINHILL_DATA_DIR` permet d'en choisir un autre.

Toutes les règles du jeu sont dans les fichiers TOML de [`data/`](data/),
embarqués dans le binaire. Pour les modifier sans recompiler, place des
fichiers du même nom dans le sous-dossier `mods/` du dossier de données :

- `balance.toml` et `crew.toml` se surchargent **clé par clé** ;
- les listes (locomotives, recherches, pays…) **remplacent ou ajoutent** les
  entrées qui portent le même identifiant.

Les données sont validées au chargement : une erreur indique le fichier et la
valeur en cause.

## Développement

```sh
cargo test
cargo clippy --all-targets
cargo fmt
```

La simulation (`src/sim/`) est pure et déterministe : elle ne touche ni au
terminal, ni au disque, ni à l'horloge. L'interface (`src/ui/`) repose sur
[ratatui](https://ratatui.rs).

## Licence

MIT.
