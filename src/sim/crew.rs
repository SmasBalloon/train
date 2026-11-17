//! Équipages : conducteurs et contrôleurs, leurs compétences et leur carrière.

use serde::{Deserialize, Serialize};

use crate::data::{CrewRules, GameData};

use super::actions::ActionError;
use super::world::World;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrewRole {
    /// Conducteur (mécanicien aux temps de la vapeur).
    Driver,
    /// Contrôleur (chef de train).
    Conductor,
}

impl CrewRole {
    pub fn name(self) -> &'static str {
        match self {
            CrewRole::Driver => "Conducteur",
            CrewRole::Conductor => "Contrôleur",
        }
    }

    /// Noms des trois compétences du métier.
    pub fn skills(self) -> [&'static str; 3] {
        match self {
            CrewRole::Driver => ["Conduite", "Économie", "Douceur"],
            CrewRole::Conductor => ["Accueil", "Contrôle", "Embarquement"],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrewMember {
    pub id: u32,
    pub name: String,
    pub role: CrewRole,
    /// Trois compétences de 0 à 1, dans l'ordre de `CrewRole::skills`.
    pub skills: [f64; 3],
    /// Kilomètres parcourus en service.
    pub km: f64,
    /// Train auquel l'équipier est affecté.
    pub train: Option<u32>,
}

impl CrewMember {
    pub fn average(&self) -> f64 {
        self.skills.iter().sum::<f64>() / 3.0
    }

    /// Salaire horaire à la première époque, selon la compétence.
    pub fn base_wage(&self, rules: &CrewRules) -> f64 {
        rules.base_wage + rules.skill_wage * self.average()
    }
}

/// Compétences de l'équipage d'un train ; zéro là où personne n'est affecté.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TrainCrew {
    pub driver: [f64; 3],
    pub conductor: [f64; 3],
    /// Vrai quand un conducteur est affecté : sans lui, le train ne part pas.
    #[serde(default)]
    pub has_driver: bool,
}

impl TrainCrew {
    pub fn driving(&self) -> f64 {
        self.driver[0]
    }
    pub fn economy(&self) -> f64 {
        self.driver[1]
    }
    pub fn care(&self) -> f64 {
        self.driver[2]
    }
    pub fn service(&self) -> f64 {
        self.conductor[0]
    }
    pub fn control(&self) -> f64 {
        self.conductor[1]
    }
    pub fn boarding(&self) -> f64 {
        self.conductor[2]
    }
}

/// Générateur déterministe (SplitMix64) : mêmes candidats pour un même jour.
fn next(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D4_9EB5_78E2_3D29);
    ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}

fn pick<'a>(list: &'a [String], seed: &mut u64) -> &'a str {
    if list.is_empty() {
        "?"
    } else {
        &list[(next(seed) * list.len() as f64) as usize % list.len()]
    }
}

impl World {
    /// Échelle des salaires : ils suivent la taille de l'économie de l'époque.
    pub fn wage_scale(&self, data: &GameData) -> f64 {
        let era = self.era(data);
        era.demand_mult * era.cost_mult
    }

    /// Salaire horaire actuel d'un équipier.
    pub fn wage_of(&self, data: &GameData, member: &CrewMember) -> f64 {
        member.base_wage(&data.crew) * self.wage_scale(data)
    }

    /// Prime à verser pour embaucher un candidat.
    pub fn hire_fee(&self, data: &GameData, member: &CrewMember) -> f64 {
        self.wage_of(data, member) * data.crew.hire_fee_hours
    }

    /// Renouvelle la liste des candidats si le jour a changé.
    pub(crate) fn refresh_candidates(&mut self, data: &GameData) {
        let day = self.day();
        if self.candidates_day == day {
            return;
        }
        self.candidates_day = day;
        let mut seed =
            day.wrapping_mul(0xA24B_AED4_963E_E407) ^ ((self.prestige.resets as u64) << 48);
        self.candidates = (0..data.crew.candidates_per_day)
            .map(|i| {
                let role = if i % 2 == 0 {
                    CrewRole::Driver
                } else {
                    CrewRole::Conductor
                };
                // Le carré favorise les débutants : les très bons équipiers sont rares.
                let mut skill = || (0.1 + 0.85 * next(&mut seed).powi(2)).min(1.0);
                let skills = [skill(), skill(), skill()];
                let first = pick(&data.crew.names.first, &mut seed).to_string();
                let last = pick(&data.crew.names.last, &mut seed);
                CrewMember {
                    id: 0,
                    name: format!("{first} {last}"),
                    role,
                    skills,
                    km: 0.0,
                    train: None,
                }
            })
            .collect();
    }

    /// Reporte sur chaque train les compétences de l'équipage qui lui est affecté.
    pub(crate) fn sync_crew(&mut self) {
        for train in &mut self.trains {
            train.crew = TrainCrew::default();
        }
        for member in &mut self.crew {
            let Some(train) = member
                .train
                .and_then(|id| self.trains.iter_mut().find(|t| t.id == id))
            else {
                member.train = None;
                continue;
            };
            match member.role {
                CrewRole::Driver => {
                    train.crew.driver = member.skills;
                    train.crew.has_driver = true;
                }
                CrewRole::Conductor => train.crew.conductor = member.skills,
            }
        }
    }

    /// Engage d'office un conducteur débutant pour un train qui n'en a pas
    /// (machine de départ, sauvegardes d'avant le conducteur obligatoire).
    pub(crate) fn novice_driver(&mut self, data: &GameData, train: u32) -> u32 {
        let mut seed = (train as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5;
        let first = pick(&data.crew.names.first, &mut seed).to_string();
        let last = pick(&data.crew.names.last, &mut seed);
        self.next_crew_id += 1;
        self.crew.push(CrewMember {
            id: self.next_crew_id,
            name: format!("{first} {last}"),
            role: CrewRole::Driver,
            skills: [0.0; 3],
            km: 0.0,
            train: Some(train),
        });
        self.sync_crew();
        self.next_crew_id
    }

    /// Premier équipier de ce métier en réserve, s'il y en a un.
    pub fn reserve(&self, role: CrewRole) -> Option<&CrewMember> {
        self.crew
            .iter()
            .filter(|m| m.role == role && m.train.is_none())
            .max_by(|a, b| a.average().total_cmp(&b.average()))
    }

    /// Prix d'une formation dans une compétence, ou `None` si elle est déjà parfaite.
    pub fn training_cost(&self, data: &GameData, member: &CrewMember, skill: usize) -> Option<f64> {
        let level = *member.skills.get(skill)?;
        if level >= 1.0 - 1e-9 {
            return None;
        }
        let rules = &data.crew;
        Some(
            rules.training_cost
                * (1.0 + rules.training_cost_growth * level * level)
                * self.wage_scale(data),
        )
    }

    /// Paie une formation à un équipier ; renvoie son prix.
    pub fn train_crew_skill(
        &mut self,
        data: &GameData,
        crew_id: u32,
        skill: usize,
    ) -> Result<f64, ActionError> {
        let member = self.crew_member(crew_id).ok_or(ActionError::UnknownCrew)?;
        if skill >= 3 {
            return Err(ActionError::UnknownCrew);
        }
        let cost = self
            .training_cost(data, member, skill)
            .ok_or(ActionError::MaxLevel)?;
        if self.money < cost {
            return Err(ActionError::NotEnoughMoney { needed: cost });
        }
        self.money -= cost;
        if let Some(member) = self.crew.iter_mut().find(|m| m.id == crew_id) {
            member.skills[skill] = (member.skills[skill] + data.crew.training_gain).min(1.0);
        }
        self.sync_crew();
        Ok(cost)
    }

    pub fn crew_member(&self, id: u32) -> Option<&CrewMember> {
        self.crew.iter().find(|m| m.id == id)
    }

    /// Équipier d'un métier affecté à un train.
    pub fn crew_on(&self, train: u32, role: CrewRole) -> Option<&CrewMember> {
        self.crew
            .iter()
            .find(|m| m.train == Some(train) && m.role == role)
    }

    /// Embauche le candidat de rang `index` ; renvoie son identifiant.
    pub fn hire_crew(&mut self, data: &GameData, index: usize) -> Result<u32, ActionError> {
        let candidate = self.candidates.get(index).ok_or(ActionError::UnknownCrew)?;
        let fee = self.hire_fee(data, candidate);
        if self.money < fee {
            return Err(ActionError::NotEnoughMoney { needed: fee });
        }
        self.money -= fee;
        let mut member = self.candidates.remove(index);
        self.next_crew_id += 1;
        member.id = self.next_crew_id;
        self.crew.push(member);
        Ok(self.next_crew_id)
    }

    /// Affecte un équipier à un train (ou le met en réserve avec `None`). Celui
    /// qui tenait ce poste sur ce train retourne en réserve.
    pub fn assign_crew(&mut self, crew_id: u32, train: Option<u32>) -> Result<(), ActionError> {
        let role = self
            .crew_member(crew_id)
            .ok_or(ActionError::UnknownCrew)?
            .role;
        if let Some(train_id) = train {
            if self.train(train_id).is_none() {
                return Err(ActionError::UnknownTrain);
            }
            for other in self
                .crew
                .iter_mut()
                .filter(|m| m.train == Some(train_id) && m.role == role)
            {
                other.train = None;
            }
        }
        if let Some(member) = self.crew.iter_mut().find(|m| m.id == crew_id) {
            member.train = train;
        }
        self.sync_crew();
        Ok(())
    }

    /// Licencie un équipier.
    pub fn dismiss_crew(&mut self, crew_id: u32) -> Result<(), ActionError> {
        let index = self
            .crew
            .iter()
            .position(|m| m.id == crew_id)
            .ok_or(ActionError::UnknownCrew)?;
        self.crew.remove(index);
        self.sync_crew();
        Ok(())
    }

    /// Masse salariale des équipages, par heure de jeu.
    pub fn crew_wages_per_hour(&self, data: &GameData) -> f64 {
        self.crew.iter().map(|m| self.wage_of(data, m)).sum()
    }

    /// Fait progresser l'équipage d'un train qui vient de parcourir `km`.
    pub(crate) fn train_crew_experience(&mut self, rules: &CrewRules, train: u32, km: f64) {
        for member in self.crew.iter_mut().filter(|m| m.train == Some(train)) {
            member.km += km;
            let gain = (rules.xp_per_km * km).min(1.0);
            for skill in &mut member.skills {
                // Progression de plus en plus lente à l'approche de la perfection.
                *skill = (*skill + (1.0 - *skill) * gain).min(1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{SimEvent, advance};

    const LINE: &str = "stockton_darlington";

    fn setup() -> (World, GameData) {
        let mut data = GameData::embedded().unwrap();
        data.events.clear();
        let mut world = World::new(&data);
        advance(&mut world, &data, 10.0);
        (world, data)
    }

    fn member(world: &mut World, role: CrewRole, skills: [f64; 3]) -> u32 {
        world.next_crew_id += 1;
        let id = world.next_crew_id;
        world.crew.push(CrewMember {
            id,
            name: "Test".into(),
            role,
            skills,
            km: 0.0,
            train: None,
        });
        id
    }

    #[test]
    fn candidates_are_the_same_for_a_given_day_and_change_the_next() {
        let (world, data) = setup();
        let (other, _) = setup();
        assert_eq!(world.candidates.len(), 4);
        assert_eq!(world.candidates, other.candidates, "tirage déterministe");
        let drivers = world
            .candidates
            .iter()
            .filter(|c| c.role == CrewRole::Driver)
            .count();
        assert_eq!(drivers, 2);
        for c in &world.candidates {
            assert!(c.skills.iter().all(|s| (0.1..=1.0).contains(s)), "{c:?}");
            assert!(c.name.contains(' ') && c.train.is_none());
        }
        let mut later = world.clone();
        advance(&mut later, &data, 86_400.0);
        assert_ne!(later.candidates, world.candidates);
    }

    #[test]
    fn hiring_costs_a_fee_and_better_people_cost_more() {
        let (mut world, data) = setup();
        let fees: Vec<f64> = world
            .candidates
            .iter()
            .map(|c| world.hire_fee(&data, c))
            .collect();
        let best = (0..4)
            .max_by(|a, b| {
                world.candidates[*a]
                    .average()
                    .total_cmp(&world.candidates[*b].average())
            })
            .unwrap();
        assert!(fees.iter().all(|f| *f <= fees[best]));
        assert!(matches!(
            world.hire_crew(&data, best),
            Err(ActionError::NotEnoughMoney { .. })
        ));
        world.money = 10_000.0;
        let name = world.candidates[best].name.clone();
        let id = world.hire_crew(&data, best).unwrap();
        assert!((world.money - (10_000.0 - fees[best])).abs() < 1e-9);
        assert_eq!(
            (world.candidates.len(), world.crew.len()),
            (3, 2),
            "fondateur + recrue"
        );
        assert_eq!(world.crew_member(id).unwrap().name, name);
        assert_eq!(world.hire_crew(&data, 9), Err(ActionError::UnknownCrew));
    }

    #[test]
    fn one_driver_and_one_conductor_per_train() {
        let (mut world, data) = setup();
        let a = member(&mut world, CrewRole::Driver, [0.5; 3]);
        let b = member(&mut world, CrewRole::Driver, [0.9; 3]);
        let c = member(&mut world, CrewRole::Conductor, [0.4; 3]);
        world.assign_crew(a, Some(1)).unwrap();
        world.assign_crew(c, Some(1)).unwrap();
        assert_eq!(
            world.trains[0].crew,
            TrainCrew {
                driver: [0.5; 3],
                conductor: [0.4; 3],
                has_driver: true,
            }
        );
        // Un second conducteur sur le même train renvoie le premier en réserve.
        world.assign_crew(b, Some(1)).unwrap();
        assert_eq!(world.crew_member(a).unwrap().train, None);
        assert_eq!(world.crew_on(1, CrewRole::Driver).unwrap().id, b);
        assert_eq!(world.trains[0].crew.driving(), 0.9);
        assert_eq!(
            world.assign_crew(a, Some(99)),
            Err(ActionError::UnknownTrain)
        );
        assert_eq!(
            world.assign_crew(99, Some(1)),
            Err(ActionError::UnknownCrew)
        );

        world.dismiss_crew(b).unwrap();
        assert_eq!(world.trains[0].crew.driver, [0.0; 3]);
        // Vendre le train remet son équipage en réserve.
        world.sell_train(&data, 1).unwrap();
        assert_eq!(world.crew_member(c).unwrap().train, None);
    }

    #[test]
    fn a_skilled_driver_is_faster_thriftier_and_gentler() {
        let run = |skills: Option<[f64; 3]>| {
            let (mut world, data) = setup();
            if let Some(skills) = skills {
                let id = member(&mut world, CrewRole::Driver, skills);
                world.assign_crew(id, Some(1)).unwrap();
            }
            world.dispatch(&data, 1).unwrap();
            let duration = world.trains[0].trip.unwrap().profile.duration_s();
            let money = world.money;
            advance(&mut world, &data, 4000.0);
            (
                duration,
                money - world.money,
                1.0 - world.lines[LINE].condition,
            )
        };
        let (plain, ace) = (run(None), run(Some([1.0; 3])));
        assert!(
            ace.0 < plain.0,
            "trajet plus court : {} contre {}",
            ace.0,
            plain.0
        );
        assert!(
            ace.2 < plain.2 * 0.75,
            "voie ménagée : {} contre {}",
            ace.2,
            plain.2
        );
        // Un conducteur qui ne sait que conduire n'économise ni charbon ni voie.
        let racer = run(Some([1.0, 0.0, 0.0]));
        assert!(
            (racer.2 / plain.2 - 1.0).abs() < 0.01,
            "même usure : {} contre {}",
            racer.2,
            plain.2
        );
        assert!(racer.0 == ace.0);
    }

    #[test]
    fn a_skilled_conductor_fills_trains_faster_and_collects_more() {
        let run = |skills: Option<[f64; 3]>| {
            let (mut world, mut data) = setup();
            data.balance.demand_per_1000_per_h = 0.0;
            if let Some(skills) = skills {
                let id = member(&mut world, CrewRole::Conductor, skills);
                world.assign_crew(id, Some(1)).unwrap();
            }
            world
                .stations
                .get_mut("stockton")
                .unwrap()
                .waiting
                .insert("tourist".into(), 100.0);
            advance(&mut world, &data, 5.0 * 60.0);
            let boarded = world.trains[0].count();
            advance(&mut world, &data, 10.0 * 60.0);
            world.dispatch(&data, 1).unwrap();
            let events = advance(&mut world, &data, 4000.0);
            let revenue = events
                .iter()
                .find_map(|e| match e {
                    SimEvent::Arrived { revenue, .. } => Some(*revenue),
                    _ => None,
                })
                .unwrap();
            (boarded, revenue, world.satisfaction_of("tourist"))
        };
        let (plain, ace) = (run(None), run(Some([1.0; 3])));
        assert_eq!((plain.0, ace.0), (20, 30), "embarquement 50 % plus rapide");
        assert!((ace.1 / plain.1 - 1.1).abs() < 1e-9, "recettes +10 %");
        assert!(ace.2 > plain.2, "les touristes apprécient l'accueil");
    }

    #[test]
    fn wages_are_paid_every_hour_and_follow_the_era() {
        let (mut world, data) = setup();
        world.crew.clear();
        let id = member(&mut world, CrewRole::Driver, [0.5; 3]);
        // 3 de base + 9 × 0,5 de compétence.
        assert_eq!(world.wage_of(&data, world.crew_member(id).unwrap()), 7.5);
        let before = world.money;
        advance(&mut world, &data, 10.0 * 3600.0);
        assert!(
            (before - world.money - 75.0).abs() < 1e-6,
            "payé même en réserve"
        );
        world.era = "steam".into();
        let wage = world.wage_of(&data, world.crew_member(id).unwrap());
        assert!((wage - 7.5 * 3.0 * 1.3).abs() < 1e-9, "{wage}");
    }

    #[test]
    fn crew_improves_with_the_kilometres() {
        let (mut world, data) = setup();
        world.research_done.insert("hand_signals".into());
        let driver = member(&mut world, CrewRole::Driver, [0.2, 0.5, 1.0]);
        let reserve = member(&mut world, CrewRole::Driver, [0.2; 3]);
        world.assign_crew(driver, Some(1)).unwrap();
        world.money = 1e6;
        advance(&mut world, &data, 6.0 * 86_400.0);
        let m = world.crew_member(driver).unwrap();
        assert!(m.km > 800.0, "{}", m.km);
        assert!(
            m.skills[0] > 0.21 && m.skills[1] > 0.5 && m.skills[2] == 1.0,
            "{:?}",
            m.skills
        );
        assert!(m.skills[0] < 1.0);
        // Le train reprend les compétences de son équipage au pas de simulation suivant.
        let gap = (world.trains[0].crew.driving() - m.skills[0]).abs();
        assert!(gap < 1e-4, "le train suit les progrès : écart {gap}");
        assert_eq!(
            world.crew_member(reserve).unwrap().skills,
            [0.2; 3],
            "la réserve n'apprend rien"
        );
    }

    #[test]
    fn training_raises_a_skill_and_gets_dearer_as_it_improves() {
        let (mut world, data) = setup();
        let id = member(&mut world, CrewRole::Driver, [0.0, 0.5, 0.95]);
        let m = world.crew_member(id).unwrap().clone();
        let (novice, half) = (
            world.training_cost(&data, &m, 0).unwrap(),
            world.training_cost(&data, &m, 1).unwrap(),
        );
        assert_eq!(novice, 60.0);
        assert_eq!(half, 60.0 * (1.0 + 6.0 * 0.25));
        assert!(matches!(
            world.train_crew_skill(&data, id, 1),
            Err(ActionError::NotEnoughMoney { .. })
        ));
        world.money = 1000.0;
        assert_eq!(world.train_crew_skill(&data, id, 1), Ok(half));
        assert_eq!(world.money, 1000.0 - half);
        assert!((world.crew_member(id).unwrap().skills[1] - 0.6).abs() < 1e-9);
        // Plafonnée à la perfection, puis plus rien à apprendre.
        world.train_crew_skill(&data, id, 2).unwrap();
        assert_eq!(world.crew_member(id).unwrap().skills[2], 1.0);
        assert_eq!(
            world.train_crew_skill(&data, id, 2),
            Err(ActionError::MaxLevel)
        );
        assert_eq!(
            world.train_crew_skill(&data, 99, 0),
            Err(ActionError::UnknownCrew)
        );
        // Un conducteur formé coûte plus cher à l'heure.
        assert!(world.wage_of(&data, world.crew_member(id).unwrap()) > world.wage_of(&data, &m));
    }

    #[test]
    fn old_saves_give_each_train_a_novice_driver() {
        let (mut world, data) = setup();
        world.crew.clear();
        world.version = 6;
        world.reconcile(&data);
        assert!(world.trains[0].crew.has_driver);
        let driver = world.crew_on(1, CrewRole::Driver).unwrap();
        assert_eq!(driver.skills, [0.0; 3]);
        // Une sauvegarde récente n'est pas touchée : le train attend un conducteur.
        world.crew.clear();
        world.reconcile(&data);
        assert!(!world.trains[0].crew.has_driver);
    }
}
