//! Dispatch takes messages sent from our wonderful UI and runs the needed core
//! code to generate the response. Essentially, it's the RPC endpoint for core.
//!
//! Each message sent in is in the following format (JSON):
//! 
//!     ["<message id>", "<command>", arg1, arg2, ...]
//!
//! where the arg\* can be any valid JSON object. The Message ID is passed in
//! when responding so the client knows which request we are responding to.

use ::jedi::{self, Value};

use ::error::{TResult, TError};
use ::config;
use ::util;
use ::util::event::Emitter;
use ::turtl::Turtl;
use ::search::Query;
use ::profile::{Profile, Export, ImportMode};
use ::models::model::Model;
use ::models::protected::Protected;
use ::models::user::User;
use ::models::space::Space;
use ::models::space_member::SpaceMember;
use ::models::note::Note;
use ::models::invite::{Invite, InviteRequest};
use ::models::file::FileData;
use ::models::sync_record::{SyncAction, SyncType, SyncRecord};
use ::models::feedback::Feedback;
use ::clippo::{self, CustomParser};
use ::sync::sync_model;
use ::sync;
use ::messaging::{self, Event};

/// Does our actual message dispatching
fn dispatch(cmd: &String, turtl: &Turtl, data: Value) -> TResult<Value> {
    match cmd.as_ref() {
        "user:login" => {
            let username = jedi::get(&["2"], &data)?;
            let password = jedi::get(&["3"], &data)?;
            turtl.login(username, password)?;
            Ok(Value::String(turtl.user_id()?))
        }
        "user:join" => {
            let username = jedi::get(&["2"], &data)?;
            let password = jedi::get(&["3"], &data)?;
            turtl.join(username, password)?;
            Ok(jedi::obj())
        }
        "user:logout" => {
            turtl.logout()?;
            util::sleep(1000);
            Ok(jedi::obj())
        }
        "user:change-password" => {
            let current_username = jedi::get(&["2"], &data)?;
            let current_password = jedi::get(&["3"], &data)?;
            let new_username = jedi::get(&["4"], &data)?;
            let new_password = jedi::get(&["5"], &data)?;
            turtl.change_user_password(current_username, current_password, new_username, new_password)?;
            Ok(jedi::obj())
        }
        "user:delete-account" => {
            turtl.delete_account()?;
            Ok(jedi::obj())
        }
        "user:find-by-email" => {
            let email: String = jedi::get(&["2"], &data)?;
            let user = User::find_by_email(turtl, &email)?;
            Ok(jedi::to_val(&user)?)
        }
        "app:connected" => {
            let connguard = turtl.connected.read().unwrap();
            let connected: bool = *connguard;
            drop(connguard);
            Ok(Value::Bool(connected))
        }
        "app:wipe-user-data" => {
            turtl.wipe_user_data()?;
            Ok(jedi::obj())
        }
        "app:wipe-app-data" => {
            turtl.wipe_app_data()?;
            Ok(jedi::obj())
        }
        "sync:start" => {
            turtl.sync_start()?;
            Ok(jedi::obj())
        }
        "sync:pause" => {
            turtl.sync_pause();
            Ok(jedi::obj())
        }
        "sync:resume" => {
            turtl.sync_resume();
            Ok(jedi::obj())
        }
        "sync:status" => {
            Ok(Value::Bool(turtl.sync_running()))
        }
        "sync:shutdown" => {
            turtl.sync_shutdown(true)?;
            Ok(jedi::obj())
        }
        "sync:get-pending" => {
            let frozen = SyncRecord::get_all_pending(turtl)?;
            Ok(jedi::to_val(&frozen)?)
        }
        "sync:unfreeze-item" => {
            let sync_id: String = jedi::get(&["2"], &data)?;
            SyncRecord::kick_frozen_sync(turtl, &sync_id)?;
            Ok(jedi::obj())
        }
        "sync:delete-item" => {
            let sync_id: String = jedi::get(&["2"], &data)?;
            SyncRecord::delete_sync_item(turtl, &sync_id)?;
            Ok(jedi::obj())
        }
        "app:api:set-endpoint" => {
            let endpoint: String = jedi::get(&["2"], &data)?;
            config::set(&["api", "endpoint"], &endpoint)?;
            Ok(jedi::obj())
        }
        "app:shutdown" => {
            turtl.sync_shutdown(false)?;
            turtl.events.trigger("app:shutdown", &jedi::obj());
            Ok(jedi::obj())
        }
        "profile:load" => {
            let user_guard = turtl.user.read().unwrap();
            let profile_guard = turtl.profile.read().unwrap();
            let profile_data = json!({
                "user": &user_guard.as_ref(),
                "spaces": &profile_guard.spaces,
                "boards": &profile_guard.boards,
                "invites": &profile_guard.invites,
            });
            Ok(profile_data)
        }
        "profile:sync:model" => {
            let action: SyncAction = match jedi::get(&["2"], &data) {
                Ok(action) => action,
                Err(e) => return TErr!(TError::BadValue(format!("bad sync action: {}", e))),
            };
            let ty: SyncType = jedi::get(&["3"], &data)?;
            let modeldata: Value = jedi::get(&["4"], &data)?;
            // construct a sync record and hand to our sync dispatcher
            let mut sync_record = SyncRecord::default();
            sync_record.action = action;
            sync_record.ty = ty;
            sync_record.data = Some(modeldata);
            sync_model::dispatch(turtl, sync_record)
        }
        "profile:space:set-owner" => {
            let space_id = jedi::get(&["2"], &data)?;
            let user_id = jedi::get(&["3"], &data)?;
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut space = match Profile::finder(&mut profile_guard.spaces, &space_id) {
                Some(s) => s,
                None => return TErr!(TError::MissingData(format!("couldn't find space {}", space_id))),
            };
            space.set_owner(turtl, &user_id)?;
            Ok(space.data()?)
        }
        "profile:space:edit-member" => {
            let mut member: SpaceMember = jedi::get(&["2"], &data)?;
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut space = match Profile::finder(&mut profile_guard.spaces, &member.space_id) {
                Some(s) => s,
                None => return TErr!(TError::MissingData(format!("couldn't find space {}", member.space_id))),
            };
            space.edit_member(turtl, &mut member)?;
            Ok(space.data()?)
        }
        "profile:space:delete-member" => {
            let space_id: String = jedi::get(&["2"], &data)?;
            let user_id: String = jedi::get(&["3"], &data)?;
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut space = match Profile::finder(&mut profile_guard.spaces, &space_id) {
                Some(s) => s,
                None => return TErr!(TError::MissingData(format!("couldn't find space {}", space_id))),
            };
            space.delete_member(turtl, &user_id)?;
            Ok(space.data()?)
        }
        "profile:space:leave" => {
            let space_id: String = jedi::get(&["2"], &data)?;
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut space = match Profile::finder(&mut profile_guard.spaces, &space_id) {
                Some(s) => s,
                None => return TErr!(TError::MissingData(format!("couldn't find space {}", space_id))),
            };
            space.leave(turtl)?;
            Ok(space.data()?)
        }
        "profile:space:send-invite" => {
            let req: InviteRequest = jedi::get(&["2"], &data)?;
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut space = match Profile::finder(&mut profile_guard.spaces, &req.space_id) {
                Some(s) => s,
                None => return TErr!(TError::MissingData(format!("couldn't find space {}", req.space_id))),
            };
            space.send_invite(turtl, req)?;
            Ok(space.data()?)
        }
        "profile:space:accept-invite" => {
            let mut invite: Invite = jedi::get(&["2"], &data)?;
            let passphrase: Option<String> = jedi::get_opt(&["3"], &data);
            let space = Space::accept_invite(turtl, &mut invite, passphrase)?;
            Ok(space.data()?)
        }
        "profile:space:edit-invite" => {
            let mut invite: Invite = jedi::get(&["2"], &data)?;
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut space = match Profile::finder(&mut profile_guard.spaces, &invite.space_id) {
                Some(s) => s,
                None => return TErr!(TError::MissingData(format!("couldn't find space {}", invite.space_id))),
            };
            space.edit_invite(turtl, &mut invite)?;
            Ok(space.data()?)
        }
        "profile:space:delete-invite" => {
            let space_id: String = jedi::get(&["2"], &data)?;
            let invite_id: String = jedi::get(&["3"], &data)?;
            let mut profile_guard = turtl.profile.write().unwrap();
            let mut space = match Profile::finder(&mut profile_guard.spaces, &space_id) {
                Some(s) => s,
                None => return TErr!(TError::MissingData(format!("couldn't find space {}", space_id))),
            };
            space.delete_invite(turtl, &invite_id)?;
            Ok(space.data()?)
        }
        "profile:delete-invite" => {
            let invite_id: String = jedi::get(&["2"], &data)?;
            Invite::delete_user_invite(turtl, &invite_id)?;
            Ok(jedi::obj())
        }
        "profile:get-notes" => {
            let note_ids = jedi::get(&["2"], &data)?;
            let notes: Vec<Note> = turtl.load_notes(&note_ids)?;
            Ok(jedi::to_val(&notes)?)
        }
        "profile:find-notes" => {
            let qry: Query = jedi::get(&["2"], &data)?;
            let search_guard = turtl.search.read().unwrap();
            if search_guard.is_none() {
                return TErr!(TError::MissingField(format!("turtl is missing `search` object")));
            }
            let search = search_guard.as_ref().unwrap();
            let note_ids = search.find(&qry)?;
            let notes: Vec<Note> = turtl.load_notes(&note_ids)?;
            Ok(jedi::to_val(&notes)?)
        }
        "profile:get-file" => {
            let note_id = jedi::get(&["2"], &data)?;
            let notes: Vec<Note> = turtl.load_notes(&vec![note_id])?;
            FileData::load_file(turtl, &notes[0])?;
            Ok(Value::Null)
        }
        "profile:get-tags" => {
            let space_id: String = jedi::get(&["2"], &data)?;
            let boards: Vec<String> = jedi::get(&["3"], &data)?;
            let limit: i32 = jedi::get(&["4"], &data)?;
            let search_guard = turtl.search.read().unwrap();
            if search_guard.is_none() {
                return TErr!(TError::MissingField(format!("turtl is missing `search` object")));
            }
            let search = search_guard.as_ref().unwrap();
            let tags = search.tags_by_frequency(&space_id, &boards, limit)?;
            Ok(jedi::to_val(&tags)?)
        }
        "profile:export" => {
            let export = Profile::export(turtl)?;
            Ok(jedi::to_val(&export)?)
        }
        "profile:import" => {
            let mode: ImportMode = jedi::get(&["2"], &data)?;
            let export: Export = jedi::get(&["3"], &data)?;
            Profile::import(turtl, mode, export)?;
            Ok(jedi::obj())
        }
        "feedback:send" => {
            let feedback: Feedback = jedi::get(&["2"], &data)?;
            feedback.send(turtl)?;
            Ok(jedi::obj())
        }
        "clip" => {
            let url: String = jedi::get(&["2"], &data)?;
            let custom_parsers: Vec<CustomParser> = jedi::get(&["3"], &data)?;
            let res = clippo::clip(&url, &custom_parsers)?;
            Ok(jedi::to_val(&res)?)
        }
        "ping" => {
            info!("ping!");
            Ok(Value::String(String::from("pong")))
        }
        _ => {
            TErr!(TError::MissingCommand(cmd.clone()))
        }
    }
}

/// Event dispatching. This acts as a way for parts of the app that don't have
/// access to the Turtl object to trigger events.
fn dispatch_event(cmd: &String, turtl: &Turtl, data: Value) -> TResult<()> {
    info!("dispatch::dispatch_event() -- {}", cmd);
    match cmd.as_ref() {
        "sync:connected" => {
            let yesno: bool = jedi::from_val(data)?;
            let mut connguard = turtl.connected.write().unwrap();
            *connguard = yesno;
        }
        "sync:incoming" => {
            sync::incoming::process_incoming_sync(turtl)?;
        }
        "user:edit" => {
            let mut user_guard = turtl.user.write().unwrap();
            user_guard.merge_fields(&data)?;
        }
        "user:change-password:logout" => {
            messaging::ui_event("user:change-password:logout", &jedi::obj())?;
            util::sleep(3000);
            turtl.logout()?;
        }
        "space:delete" => {
            let space_id: String = jedi::get(&["0"], &data)?;
            let skip_remote_sync: bool = match jedi::get_opt(&["1"], &data) {
                Some(x) => x,
                None => false,
            };
            sync_model::delete_model::<Space>(turtl, &space_id, skip_remote_sync)?;
        }
        _ => {
            warn!("dispatch_event() -- encountered unknown event: {}", cmd);
        }
    }
    Ok(())
}

/// process a message from the messaging system. this is the main communication
/// heart of turtl core.
pub fn process(turtl: &Turtl, msg: &String) -> TResult<()> {
    if &msg[0..4] == "::ev" {
        let event: Event = jedi::parse(&String::from(&msg[4..]))?;
        let Event {e, d} = event;
        return dispatch_event(&e, turtl, d);
    }

    let data: Value = jedi::parse(msg)?;

    // grab the request id from the data
    let mid: String = match jedi::get(&["0"], &data) {
        Ok(x) => x,
        Err(_) => return TErr!(TError::MissingField(String::from("missing mid (0)"))),
    };
    // grab the command from the data
    let cmd: String = match jedi::get(&["1"], &data) {
        Ok(x) => x,
        Err(_) => return TErr!(TError::MissingField(String::from("missing cmd (1)"))),
    };

    info!("dispatch({}): {}", mid, cmd);

    match dispatch(&cmd, turtl.clone(), data) {
        Ok(val) => {
            match turtl.msg_success(&mid, val) {
                Err(e) => error!("dispatch::process() -- problem sending response (mid {}): {}", mid, e),
                _ => {},
            }
        },
        Err(e) => {
            match turtl.msg_error(&mid, &e) {
                Err(e) => error!("dispatch:process() -- problem sending (error) response (mod {}): {}", mid, e),
                _ => {},
            }
        },
    }
    Ok(())
}

