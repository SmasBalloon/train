//! Notifications de bureau pour le mode démon.
//!
//! Le texte est construit ici ; l'envoi passe par l'outil du système
//! (`notify-send` sous Linux, `osascript` sous macOS) et n'échoue jamais :
//! sans outil disponible, la notification est simplement perdue.

use std::process::{Command, Stdio};

use crate::data::GameData;
use crate::sim::SimEvent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

impl Notice {
    fn new(title: &str, body: String) -> Notice {
        Notice {
            title: title.to_string(),
            body,
        }
    }
}

/// Notification correspondant à un fait de jeu, s'il mérite d'interrompre le joueur.
pub fn notice_for(event: &SimEvent, data: &GameData) -> Option<Notice> {
    match event {
        SimEvent::ResearchDone { id } => {
            let name = data
                .research(id)
                .map_or(id.as_str(), |r| r.name.fr.as_str());
            Some(Notice::new(
                "Recherche terminée",
                format!("{name} : tes ingénieurs attendent un nouveau projet."),
            ))
        }
        SimEvent::EventReady { id } => {
            let title = data.event(id).map_or(id.as_str(), |e| e.title.fr.as_str());
            Some(Notice::new(
                "Un événement t'attend",
                format!("{title} : la compagnie a besoin de ta décision."),
            ))
        }
        SimEvent::FuelOut { fuel } => {
            let name = data
                .fuel(fuel)
                .map_or(fuel.as_str(), |f| f.name.fr.as_str());
            Some(Notice::new(
                "Stock épuisé",
                format!("{name} : les trains roulent au prix fort."),
            ))
        }
        SimEvent::TrackWorn { line } => {
            let name = data.line(line).map_or(line.as_str(), |l| l.name.as_str());
            Some(Notice::new(
                "Voie usée",
                format!("{name} : les trains ralentissent."),
            ))
        }
        // Le quotidien de l'exploitation ne dérange personne.
        SimEvent::Departed { .. }
        | SimEvent::Arrived { .. }
        | SimEvent::Repaired { .. }
        | SimEvent::FuelBought { .. } => None,
    }
}

/// Notification annonçant que l'époque suivante est à portée.
pub fn era_notice(era_name: &str) -> Notice {
    Notice::new(
        "Nouvelle époque à portée",
        format!("{era_name} : toutes les conditions sont réunies."),
    )
}

/// Commande système qui affiche la notification, ou `None` si la plateforme n'en a pas.
fn command(notice: &Notice) -> Option<Command> {
    if cfg!(target_os = "macos") {
        // Dans une chaîne AppleScript, seuls l'antislash et le guillemet s'échappent.
        let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display notification \"{}\" with title \"Rainhill\" subtitle \"{}\"",
            quote(&notice.body),
            quote(&notice.title)
        );
        let mut cmd = Command::new("osascript");
        cmd.arg("-e").arg(script);
        Some(cmd)
    } else if cfg!(unix) {
        let mut cmd = Command::new("notify-send");
        // `--` : un titre commençant par un tiret n'est pas une option.
        cmd.args(["--app-name=Rainhill", "--"])
            .arg(&notice.title)
            .arg(&notice.body);
        Some(cmd)
    } else {
        None
    }
}

/// Envoie la notification sans attendre ni signaler d'erreur.
pub fn send(notice: &Notice) {
    let Some(mut cmd) = command(notice) else {
        return;
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(mut child) = cmd.spawn() {
        // On attend le processus dans un fil à part, pour ne pas laisser de zombie.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_noteworthy_events_notify() {
        let data = GameData::embedded().unwrap();
        let notice = notice_for(
            &SimEvent::ResearchDone {
                id: "hand_signals".into(),
            },
            &data,
        )
        .unwrap();
        assert_eq!(notice.title, "Recherche terminée");
        assert!(notice.body.starts_with("Signaleurs de voie"));

        let notice = notice_for(
            &SimEvent::EventReady {
                id: "rainhill_trials".into(),
            },
            &data,
        )
        .unwrap();
        assert!(notice.body.contains("Le concours de Rainhill"));
        let notice = notice_for(
            &SimEvent::FuelOut {
                fuel: "coal".into(),
            },
            &data,
        )
        .unwrap();
        assert!(notice.body.starts_with("Charbon"));
        let notice = notice_for(
            &SimEvent::TrackWorn {
                line: "stockton_darlington".into(),
            },
            &data,
        )
        .unwrap();
        assert!(notice.body.starts_with("Stockton & Darlington"));

        let routine = [
            SimEvent::Departed {
                train: 1,
                station: "stockton".into(),
            },
            SimEvent::Arrived {
                train: 1,
                station: "stockton".into(),
                passengers: 3,
                revenue: 1.0,
                delay_min: 0.0,
            },
            SimEvent::Repaired {
                line: "stockton_darlington".into(),
                cost: 10.0,
            },
            SimEvent::FuelBought {
                fuel: "coal".into(),
                quantity: 10.0,
                cost: 10.0,
            },
        ];
        assert!(routine.iter().all(|e| notice_for(e, &data).is_none()));
        assert!(
            era_notice("Âge d'or de la vapeur")
                .body
                .contains("Âge d'or")
        );
    }

    #[cfg(unix)]
    #[test]
    fn notification_text_is_passed_as_arguments_not_through_a_shell() {
        let notice = Notice::new("-titre \"piège\"", "corps; rm -rf $HOME `x`".to_string());
        let cmd = command(&notice).unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if cfg!(target_os = "macos") {
            assert!(args[1].contains("\\\"piège\\\""));
        } else {
            assert_eq!(
                args,
                [
                    "--app-name=Rainhill",
                    "--",
                    "-titre \"piège\"",
                    "corps; rm -rf $HOME `x`"
                ]
            );
        }
    }
}
