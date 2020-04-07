/// Handles the users of an account. Users in TERA terminology are the player characters of an account.
use std::sync::Arc;

use legion::prelude::*;
use legion::systems::schedule::Schedulable;
use legion::systems::SystemBuilder;
use tracing::{debug, info_span};

use crate::ecs::component::SingleEvent;
use crate::ecs::event::Event;
use crate::ecs::event::EventKind;
use crate::ecs::system::send_event;
use crate::ecs::tag;
use crate::model::{Class, Customization, Gender, Race, Vec3, Vec3a};
use crate::protocol::packet::*;

pub fn init(world_id: usize) -> Box<dyn Schedulable> {
    SystemBuilder::new("UserManager")
        .with_query(<Read<SingleEvent>>::query().filter(tag_value(&tag::EventKind(EventKind::Request))))
        .write_component::<SingleEvent>()
        .build(move |mut command_buffer, world, _resources, queries| {
            let span = info_span!("world", world_id);
            let _enter = span.enter();

            // TODO The user manager should listen to the "Drop Connection" event and persist the state of the user
            for event in queries.iter(&*world) {
                match &**event {
                    Event::RequestGetUserList { connection, .. } => {
                        handle_user_list(&connection, &mut command_buffer);
                    }
                    _ => { /* Ignore all other events */ }
                }
            }
        })
}

fn handle_user_list(connection: &Option<Entity>, mut command_buffer: &mut CommandBuffer) {
    debug!("Get user list event incoming");

    // TODO Just a mock. Proper DB handling comes later.
    let response = Event::ResponseGetUserList {
        connection: *connection,
        packet: SGetUserList {
            characters: vec![SGetUserListCharacter {
                custom_strings: vec![SGetUserListCharacterCustomString {
                    string: "Pantsu".to_string(),
                    id: 254_312,
                }],
                name: "Almetica".to_string(),
                details: vec![
                    0, 7, 0, 12, 0, 0, 0, 0, 26, 24, 20, 0, 0, 13, 7, 0, 16, 0, 16, 16, 0, 0, 0, 14, 17, 29, 12, 24,
                    26, 16, 7, 3,
                ],
                shape: vec![
                    1, 19, 16, 19, 19, 16, 19, 19, 19, 16, 16, 16, 16, 15, 15, 15, 16, 19, 10, 0, 22, 23, 9, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0,
                ],
                guild_name: "".to_string(),
                id: 2_000_131,
                gender: Gender::Female,
                race: Race::ElinPopori,
                class: Class::Lancer,
                level: 65,
                hp: 121_111,
                mp: 2000,
                world_id: 1,
                guard_id: 2,
                section_id: 8,
                last_logout_time: 1_584_074_481,
                is_deleting: false,
                delete_time: 86400,
                delete_remain_sec: -1_585_902_611,
                weapon: 28369,
                earring1: 96399,
                earring2: 96398,
                body: 96281,
                hand: 96283,
                feet: 96285,
                unk_item7: 0,
                ring1: 96392,
                ring2: 96391,
                underwear: 179_035,
                head: 50056,
                face: 5,
                appearance: Customization {
                    data: [0, 0, 0, 0, 0, 0, 0, 0],
                },
                is_second_character: false,
                admin_level: 0,
                is_banned: false,
                ban_end_time: 0,
                ban_remain_sec: -1_585_989_011,
                rename_needed: 0,
                weapon_model: 0,
                unk_model2: 0,
                unk_model3: 0,
                body_model: 0,
                hand_model: 0,
                feet_model: 0,
                unk_model7: 0,
                unk_model8: 0,
                unk_model9: 0,
                unk_model10: 0,
                unk_dye1: 0,
                unk_dye2: 0,
                weapon_dye: 0,
                body_dye: 0,
                hand_dye: 0,
                feet_dye: 0,
                unk_dye7: 0,
                unk_dye8: 0,
                unk_dye9: 0,
                underwear_dye: 0,
                style_back_dye: 0,
                style_head_dye: 0,
                style_face_dye: 0,
                style_head: 177_018,
                style_face: 0,
                style_back: 0,
                style_weapon: 170_029,
                style_body: 177_761,
                style_footprint: 0,
                style_body_dye: 421_075_260,
                weapon_enchant: 15,
                rest_bonus_xp: 292_832_832,
                max_rest_bonus_xp: 292_832_844,
                show_face: true,
                style_head_scale: 1.0,
                style_head_rotation: Vec3a { x: 0, y: 0, z: 0 },
                style_head_translation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
                style_head_translation_debug: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
                style_faces_scale: 1.0,
                style_face_rotation: Vec3a { x: 0, y: 0, z: 0 },
                style_face_translation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
                style_face_translation_debug: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
                style_back_scale: 1.0,
                style_back_rotation: Vec3a { x: 0, y: 0, z: 0 },
                style_back_translation: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
                style_back_translation_debug: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
                used_style_head_transform: false,
                is_new_character: false,
                tutorial_state: 0,
                show_style: true,
                appearance2: 100,
                achievement_points: 13565,
                laurel: 0,
                position: 1,
                guild_logo_id: 4521,
                awakening_level: 0,
                has_broker_sales: false,
            }],
            veteran: false,
            bonus_buf_sec: 0,
            max_characters: 12,
            first: true,
            more: false,
            left_del_time_account_over: 0,
            deletion_section_classify_level: 40,
            delete_character_expire_hour1: 0,
            delete_character_expire_hour2: 24,
        },
    };

    send_event(Arc::new(response), &mut command_buffer);
}
