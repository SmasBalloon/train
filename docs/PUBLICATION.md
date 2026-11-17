# Publier Rainhill

Tout ce qu'il faut faire pour publier le jeu : la mise en place (une seule
fois), puis la marche à suivre à chaque nouvelle version.

Le jeu est distribué par cinq canaux, tous mis à jour par un seul lancement du
workflow **Release** :

| Canal               | Le joueur tape                 | Mis à jour par                        |
|---------------------|--------------------------------|---------------------------------------|
| GitHub Releases     | (téléchargement de l'archive)  | le workflow **Release**, automatique  |
| crates.io           | `cargo install rainhill`       | le workflow **Release**, automatique  |
| AUR (Arch Linux)    | `yay -S rainhill-bin`          | le workflow **AUR**, automatique      |
| Homebrew (macOS, Linux) | `brew tap smasballoon/rainhill https://github.com/SmasBalloon/train` puis `brew install smasballoon/rainhill/rainhill` | le workflow **Release**, automatique |
| Scoop (Windows)     | `scoop bucket add rainhill https://github.com/SmasBalloon/train` puis `scoop install rainhill/rainhill` | le workflow **Release**, automatique |

Pas de dépôt séparé pour Homebrew ni Scoop : ce dépôt sert lui-même de tap
Homebrew (`Formula/rainhill.rb`) et de bucket Scoop (`bucket/rainhill.json`).
Le workflow **Release** régénère ces deux fichiers et les inclut dans son
commit de version.

---

## 1. Mise en place (une seule fois)

### Déjà fait

- [x] Licence (`LICENSE`), guide de contribution (`CONTRIBUTING.md`)
- [x] Métadonnées crates.io dans `Cargo.toml`
- [x] Workflows **CI** et **Release** sur GitHub
- [x] Compte crates.io, e-mail vérifié, jeton créé et `cargo login` fait
- [x] Release **v1.0.0** publiée sur GitHub et sur crates.io

### À faire maintenant

- [ ] **Commiter et pousser les publications automatiques** (crates.io, AUR, Homebrew, Scoop)

  ```sh
  git add .github packaging docs Formula bucket Cargo.toml README.md
  git commit -s -m "ci: publication automatique sur crates.io, l'AUR, Homebrew et Scoop"
  git push
  ```

- [x] **Autoriser GitHub à publier sur crates.io** (Trusted Publishing : aucun
  jeton à stocker, crates.io vérifie que la demande vient de ton workflow).
  Sur <https://crates.io/crates/rainhill> → **Settings** → **Trusted Publishing**
  → **Add** → **GitHub** :
  - **Repository owner** : `SmasBalloon`
  - **Repository name** : `train`
  - **Workflow filename** : `release.yml`
  - **Environment** : laisser vide

- [x] Facultatif : le jeton crates.io créé pour `cargo login` ne sert plus
  qu'en secours. Tu peux le garder (pour publier à la main si besoin) ou le
  révoquer sur crates.io → **Account Settings** → **API Tokens**.

- [ ] **Créer la clé SSH réservée au CI** (jamais ta clé personnelle dans GitHub)

  ```sh
  ssh-keygen -t ed25519 -f ~/.ssh/aur_ci -N "" -C "rainhill-ci"
  ```

- [ ] **Donner la clé privée à GitHub**, sous forme de secret :

  ```sh
  gh secret set AUR_SSH_PRIVATE_KEY < ~/.ssh/aur_ci
  ```

  Vérifier avec `gh secret list` : `AUR_SSH_PRIVATE_KEY` doit apparaître.

### Homebrew et Scoop

Rien à créer ni à configurer : `Formula/rainhill.rb` et `bucket/rainhill.json`
(pour la v1.0.0) sont déjà dans le dépôt.

- [ ] Après avoir poussé, **vérifier l'installation** : GitHub → **Actions** →
  **Homebrew et Scoop** → **Run workflow**, **tag** `v1.0.0`. Le run installe
  le jeu sur un vrai Mac et un vrai Windows : les deux jobs doivent être verts.

### À faire quand l'AUR rouvrira les inscriptions

Les inscriptions sont temporairement fermées. Suivre la réouverture sur
<https://archlinux.org/news/> et <https://status.archlinux.org/>.

- [ ] **Créer le compte** sur <https://aur.archlinux.org/register>.
  Dans le champ **SSH Public Key**, coller **les deux clés, une par ligne** :

  ```sh
  cat ~/.ssh/id_ed25519.pub   # ta clé personnelle
  cat ~/.ssh/aur_ci.pub       # la clé du CI
  ```

- [ ] **Publier la v1.0.0 sur l'AUR** (elle est sortie avant l'automatisation) :
  GitHub → **Actions** → **AUR** → **Run workflow**
  - **tag** : `v1.0.0`
  - **pkgrel** : `1`

- [ ] **Vérifier** : <https://aur.archlinux.org/packages/rainhill-bin> existe, puis

  ```sh
  yay -S rainhill-bin
  rail --version
  ```

### En attendant l'AUR

Les joueurs sous Arch peuvent construire le même paquet eux-mêmes :

```sh
git clone https://github.com/SmasBalloon/train.git
cd train/packaging/aur/rainhill-bin
makepkg -si
```

---

## 2. À chaque nouvelle version

### Avant

- [ ] Tout le travail est commité et poussé sur `master`.
- [ ] Le **CI** est vert sur le dernier commit (onglet **Actions**).
- [ ] Les messages de commit suivent le format `feat:` / `fix:` (voir
  `CONTRIBUTING.md`) : ils deviennent le patch note, et décident du numéro de
  version si on ne le donne pas.

  ⚠️ Le message est **seulement le texte entre guillemets** :
  `git commit -s -m "fix: …"` donne le message `fix: …`.

### Essai à blanc (recommandé)

- [ ] GitHub → **Actions** → **Release** (colonne de gauche, pas « CI ») →
  **Run workflow** :
  - **Version** : la nouvelle version (ex. `1.1.0`), ou vide pour la déduire
    des commits ;
  - **Essai à blanc** : **coché**.
- [ ] Tout est vert, et l'étape « Publier » est **grisée** (normal).
- [ ] Relire le patch note : en bas du run, artefact **release-notes**.

### Publication

- [ ] Relancer **Release** avec les mêmes réglages, **essai à blanc décoché**.
  Le workflow, tout seul :
  1. passe `Cargo.toml` à la nouvelle version et commite ;
  2. crée et pousse le tag `vX.Y.Z` ;
  3. publie la release GitHub (6 archives + `SHA256SUMS.txt`) ;
  4. publie la version sur crates.io ;
  5. construit, teste et publie le paquet AUR `rainhill-bin` ;
  6. vérifie l'installation par Homebrew (macOS) et Scoop (Windows), dont la
     formule et le manifeste sont partis avec le commit de version (étape 1).
- [ ] Récupérer le commit de version en local, pour continuer à travailler
  dessus :

  ```sh
  git pull
  ```

  ⚠️ Une version publiée sur crates.io est **définitive** : impossible de la
  supprimer, seulement de la déconseiller (`cargo yank --version X.Y.Z`).
  D'où l'intérêt de l'essai à blanc.

### Après

- [ ] La release est en ligne : <https://github.com/SmasBalloon/train/releases>
- [ ] Le job **Publier sur crates.io** du run est vert, et crates.io affiche la
  version : <https://crates.io/crates/rainhill>
- [ ] Le job **AUR** du run est vert, et l'AUR affiche la version :
  <https://aur.archlinux.org/packages/rainhill-bin>
- [ ] Les deux vérifications **Homebrew et Scoop** (macOS et Windows) sont
  vertes.
- [ ] Facultatif : nettoyer le patch note sur GitHub (bouton ✏️ de la release).

---

## 3. En cas de problème

| Problème | Solution |
|---|---|
| **Release** refuse : « Les releases se lancent depuis master » | Choisir `master` dans « Use workflow from ». |
| « Aucun commit depuis vX.Y.Z : rien à publier » | Il n'y a rien de nouveau depuis la dernière release. |
| « Le tag vX.Y.Z existe déjà » | Cette version est déjà sortie : choisir un numéro supérieur. |
| Job **crates.io** : erreur à l'étape `crates-io-auth-action` | Trusted Publishing n'est pas configuré, ou mal : vérifier `SmasBalloon` / `train` / `release.yml` dans les réglages du paquet sur crates.io (section 1). |
| Job **crates.io** échoué, mais la release GitHub est sortie | Corriger la cause, puis publier à la main depuis le tag : `git checkout vX.Y.Z && cargo publish && git checkout master` (demande `cargo login`). |
| Job **crates.io** : « déjà sur crates.io : rien à faire » | Normal si la version a déjà été publiée (à la main, ou lors d'un run précédent). |
| Job **AUR** : « Secret AUR_SSH_PRIVATE_KEY absent » | Le paquet a été testé mais pas publié : ajouter le secret (section 1). |
| Job **AUR** : « Permission denied (publickey) » | La clé `~/.ssh/aur_ci.pub` n'est pas enregistrée sur le compte AUR. |
| Job **AUR** : « Empreinte SSH inattendue » | L'AUR a changé de clé serveur : comparer avec l'empreinte affichée sur <https://aur.archlinux.org>, puis mettre à jour `AUR_HOST_FINGERPRINT` dans `.github/workflows/aur.yml`. |
| Corriger le `PKGBUILD` sans nouvelle version du jeu | Modifier `packaging/aur/rainhill-bin/PKGBUILD`, pousser, puis lancer **AUR** à la main avec le tag actuel et **pkgrel** `2` (puis 3…). |
| Une version publiée est cassée | Sortir vite une version corrective (`fix:` → ex. `1.0.1`). Sur crates.io : `cargo yank --version 1.0.0`. |
| Vérification **brew** ou **scoop** en échec | La formule ou le manifeste est fautif : lire l'erreur, corriger `.github/scripts/package-manifests.sh`, régénérer avec `bash .github/scripts/package-manifests.sh vX.Y.Z .`, pousser, puis relancer **Homebrew et Scoop** à la main avec le même tag. |
