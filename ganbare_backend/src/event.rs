use super::*;

pub fn update_event(conn: &Connection, item: &UpdateEvent) -> Result<Option<Event>> {
    use schema::events;

    let item = diesel::update(events::table.filter(events::id.eq(item.id))).set(item)
        .get_result(&**conn)
        .optional()?;
    Ok(item)
}

pub fn get_all(conn: &Connection)
               -> Result<Vec<(Event, Vec<(User, bool, Option<EventExperience>)>)>> {
    use schema::{users, events};

    let events: Vec<Event> = events::table.order(events::priority)
        .get_results(&**conn)?;

    let users: Vec<User> = users::table.get_results(&**conn)?;

    let mut event_data = vec![];
    for event in events {

        let mut exp_data: Vec<(User, bool, Option<EventExperience>)> = vec![];
        for user in &users {

            match is_workable_or_done_by_event_id(conn, event.id, user)? {
                Some((_, Some(exp))) => exp_data.push((user.clone(), true, Some(exp))),
                Some((_, None)) => exp_data.push((user.clone(), true, None)),
                None => exp_data.push((user.clone(), false, None)),
            }
        }
        event_data.push((event, exp_data));
    }

    Ok(event_data)
}

pub fn dispatch_event(conn: &Connection, user_id: i32) -> Result<Option<Event>> {
    use schema::{events, event_experiences, group_memberships};
    use diesel::expression::dsl::{any, all};

    let groups = group_memberships::table.filter(group_memberships::user_id.eq(user_id))
        .select(group_memberships::group_id.nullable());

    let finished_events = event_experiences::table.filter(event_experiences::user_id.eq(user_id))
        .filter(event_experiences::event_finish.is_not_null())
        .select(event_experiences::event_id);

    Ok(events::table.filter(events::published.eq(true))
        .filter(events::required_group.eq(any(groups)).or(events::required_group.is_null()))
        .filter(events::id.ne(all(finished_events)))
        .order(events::priority.asc())
        .first(&**conn)
        .optional()?)
}

pub fn is_workable_or_done_by_event_id(conn: &Connection,
                                       event_id: i32,
                                       user: &User)
                                       -> Result<Option<(Event, Option<EventExperience>)>> {
    use schema::{events, event_experiences, group_memberships};

    let event_exp: Option<Event> = events::table.filter(events::id.eq(event_id))
        .get_result(&**conn)
        .optional()?;

    let event = if let Some(e) = event_exp {
        e
    } else {
        return Ok(None);
    };

    let exp: Option<EventExperience> =
        event_experiences::table.filter(event_experiences::user_id.eq(user.id))
            .filter(event_experiences::event_id.eq(event.id))
            .get_result(&**conn)
            .optional()?;

    let group_id = if let Some(g) = event.required_group {
        g
    } else {
        return Ok(Some((event, exp)));
    };


    let group_membership: Option<GroupMembership> =
        group_memberships::table.filter(group_memberships::group_id.eq(group_id))
            .filter(group_memberships::user_id.eq(user.id))
            .get_result(&**conn)
            .optional()?;

    match group_membership {
        Some(_) => Ok(Some((event, exp))),
        None => Ok(None),
    }
}

pub fn is_workable(conn: &Connection, event_name: &str, user: &User) -> Result<Option<Event>> {
    use schema::{events, event_experiences, group_memberships};

    let event: Event = events::table.filter(events::name.eq(event_name))
        .get_result(&**conn)
        .chain_err(|| ErrorKind::Msg(format!("Error fetching event {:?}", event_name)))?;

    if !event.published {
        return Ok(None);
    };

    let exp: Option<EventExperience> =
        event_experiences::table.filter(event_experiences::user_id.eq(user.id))
            .filter(event_experiences::event_id.eq(event.id))
            .get_result(&**conn)
            .optional()?;

    if let Some(exp) = exp {
        if exp.event_finish.is_some() {
            return Ok(None); // The event is already done
        }
    }

    let group_id = if let Some(g) = event.required_group {
        g
    } else {
        return Ok(Some(event));
    };

    let group_membership: Option<GroupMembership> =
        group_memberships::table.filter(group_memberships::group_id.eq(group_id))
            .filter(group_memberships::user_id.eq(user.id))
            .get_result(&**conn)
            .optional()?;

    match group_membership {
        Some(_) => Ok(Some(event)),
        None => Ok(None),
    }
}

pub fn state(conn: &Connection,
             event_name: &str,
             user: &User)
             -> Result<Option<(Event, EventExperience)>> {
    use schema::{event_experiences, events};

    let event: Event = events::table.filter(events::name.eq(event_name))
        .get_result(&**conn)?;

    let ok = event_experiences::table.filter(event_experiences::user_id.eq(user.id))
        .filter(event_experiences::event_id.eq(event.id))
        .get_result(&**conn)
        .optional()?;
    Ok(ok.map(|exp| (event, exp)))
}


pub fn is_done(conn: &Connection, event_name: &str, user: &User) -> Result<bool> {
    let state = state(conn, event_name, user)?;
    Ok(match state {
        Some((_, exp)) => {
            match exp.event_finish {
                Some(_) => true,
                None => false,
            }
        }
        None => false,
    })
}


pub fn is_published(conn: &Connection, event_name: &str) -> Result<bool> {
    use schema::events;
    let ev: Event = events::table.filter(events::name.eq(event_name))
        .get_result(&**conn)?;

    Ok(ev.published)
}


pub fn initiate(conn: &Connection,
                event_name: &str,
                user: &User)
                -> Result<Option<(Event, EventExperience)>> {
    use schema::{event_experiences, events};

    if let Some((ev, exp)) = state(conn, event_name, user)? {
        return Ok(Some((ev, exp)));
    };

    let ev: Event = events::table.filter(events::name.eq(event_name))
        .get_result(&**conn)?;

    let exp: EventExperience = diesel::insert(&NewEventExperience {
            user_id: user.id,
            event_id: ev.id,
        }).into(event_experiences::table)
        .get_result(&**conn)?;

    Ok(Some((ev, exp)))
}

pub fn require_started(conn: &Connection,
                    event_name: &str,
                    user: &User)
                    -> Result<(Event, EventExperience)> {

    let ev_state = state(conn, event_name, user)?;

    if let Some(ev_exp @ (Event { published: true, .. }, .. )) = ev_state {
        Ok(ev_exp)
    } else {
        bail!(ErrorKind::AccessDenied)
    }
}

pub fn require_done(conn: &Connection,
                    event_name: &str,
                    user: &User)
                    -> Result<(Event, EventExperience)> {

    let ev_state = state(conn, event_name, user)?;

    if let Some(ev_exp @ (_, EventExperience { event_finish: Some(_), .. })) = ev_state {
        Ok(ev_exp)
    } else {
        bail!(ErrorKind::AccessDenied)
    }
}

pub fn require_ongoing(conn: &Connection,
                       event_name: &str,
                       user: &User)
                       -> Result<(Event, EventExperience)> {

    if let Some(ev_exp) = is_ongoing(conn, event_name, user)? {
        Ok(ev_exp)
    } else {
        bail!(ErrorKind::AccessDenied)
    }
}

pub fn is_ongoing(conn: &Connection,
                  event_name: &str,
                  user: &User)
                  -> Result<Option<(Event, EventExperience)>> {

    let ev_state = state(conn, event_name, user)?;

    if let Some(ev_exp @ (Event { published: true, .. }, EventExperience { event_finish: None, .. })) = ev_state {
        Ok(Some(ev_exp))
    } else {
        Ok(None)
    }
}


pub fn set_done(conn: &Connection,
                event_name: &str,
                user: &User)
                -> Result<Option<(Event, EventExperience)>> {
    use schema::event_experiences;

    if let Some((ev, mut exp)) = state(conn, event_name, user)? {
        exp.event_finish = Some(chrono::UTC::now());
        diesel::update(event_experiences::table.filter(event_experiences::event_id.eq(ev.id))
                .filter(event_experiences::user_id.eq(user.id))).set(&exp)
            .execute(&**conn)?;
        Ok(Some((ev, exp)))
    } else {
        Ok(None)
    }
}

pub fn remove_exp(conn: &Connection, event_id: i32, user_id: i32) -> Result<bool> {
    use schema::{event_userdata, event_experiences};

    let count_userdata =
        diesel::delete(event_userdata::table.filter(event_userdata::event_id.eq(event_id))
                .filter(event_userdata::user_id.eq(user_id))).execute(&**conn)?;

    let count_exp =
        diesel::delete(event_experiences::table.filter(event_experiences::event_id.eq(event_id))
                .filter(event_experiences::user_id.eq(user_id))).execute(&**conn)?;

    debug!("Removing event exp! Userdata rows: {}, exp rows: {}", count_userdata, count_exp);

    Ok(count_exp == 1)
}

pub fn save_userdata(conn: &Connection,
                     event: &Event,
                     user: &User,
                     key: Option<&str>,
                     data: &str)
                     -> Result<EventUserdata> {
    use schema::event_userdata;

    time_it!("save_userdata",
    match key {
        None => {
            Ok(diesel::insert(&NewEventUserdata {
                    event_id: event.id,
                    user_id: user.id,
                    key: key,
                    data: data,
                }).into(event_userdata::table)
                .get_result(&**conn)?)
        }
        Some(k) => {
            let result =
                diesel::update(event_userdata::table.filter(event_userdata::event_id.eq(event.id))
                        .filter(event_userdata::user_id.eq(user.id))
                        .filter(event_userdata::key.eq(k))).set(&UpdateEventUserdata { data: data })
                    .get_result(&**conn)
                    .optional()?;
            if let Some(userdata) = result {
                Ok(userdata)
            } else {
                Ok(diesel::insert(&NewEventUserdata {
                        event_id: event.id,
                        user_id: user.id,
                        key: key,
                        data: data,
                    }).into(event_userdata::table)
                    .get_result(&**conn)?)
            }
        }
    })
}

pub fn get_userdata(conn: &Connection,
                    event: &Event,
                    user: &User,
                    key: &str)
                    -> Result<Option<EventUserdata>> {
    use schema::event_userdata;

    let result: Option<EventUserdata> = time_it!("get_userdata",
    event_userdata::table.filter(event_userdata::key.eq(key))
        .filter(event_userdata::user_id.eq(user.id))
        .filter(event_userdata::event_id.eq(event.id))
        .get_result(&**conn)
        .optional()
    )?;

    Ok(result)
}
