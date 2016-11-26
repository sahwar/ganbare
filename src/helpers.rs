
use std;
use std::env;
use dotenv;
use std::net::{SocketAddr, ToSocketAddrs};
use ganbare;
use ganbare::PgConnection;
use hyper::header::{SetCookie, CookiePair, Cookie};
use std::collections::BTreeMap;
use pencil;
use pencil::{Request, Response, abort, PencilError};
use ganbare::models::{User, Session};
use ganbare::errors::{ErrorKind};
use std::net::IpAddr;
use time;
use std::result::Result as StdResult;
use ganbare::errors::Result as Result;
use rustc_serialize::base64::FromBase64;

lazy_static! {
 
    pub static ref DATABASE_URL : String = { dotenv::dotenv().ok(); env::var("GANBARE_DATABASE_URL")
        .expect("GANBARE_DATABASE_URL must be set (format: postgres://username:password@host/dbname)")};

    pub static ref SITE_DOMAIN : String = { dotenv::dotenv().ok(); env::var("GANBARE_SITE_DOMAIN")
        .expect("GANBARE_SITE_DOMAIN: Set the site domain! (Without it, the cookies don't work.)") };

    pub static ref EMAIL_SERVER : SocketAddr = { dotenv::dotenv().ok();
        let binding = env::var("GANBARE_EMAIL_SERVER")
        .expect("GANBARE_EMAIL_SERVER: Specify an outbound email server, like this: mail.yourisp.com:25");
        binding.to_socket_addrs().expect("Format: domain:port").next().expect("Format: domain:port") };

    pub static ref EMAIL_DOMAIN : String = { dotenv::dotenv().ok(); env::var("GANBARE_EMAIL_DOMAIN")
        .unwrap_or_else(|_|  env::var("GANBARE_SITE_DOMAIN").unwrap_or_else(|_| "".into())) };

    pub static ref SERVER_BINDING : SocketAddr = { dotenv::dotenv().ok();
        let binding = env::var("GANBARE_SERVER_BINDING")
        .unwrap_or_else(|_| "localhost:8080".into());
        binding.to_socket_addrs().expect("GANBARE_SERVER_BINDING: Format: domain:port").next()
        .expect("GANBARE_SERVER_BINDING: Format: domain:port") };

    pub static ref JQUERY_URL : String = { dotenv::dotenv().ok(); env::var("GANBARE_JQUERY")
        .unwrap_or_else(|_| "/static/js/jquery.min.js".into()) };

    pub static ref FONT_URL : String = { dotenv::dotenv().ok(); env::var("GANBARE_FONT_URL")
        .unwrap_or_else(|_| "/static/fonts/default.css".into()) };

    pub static ref AUDIO_DIR : String = { dotenv::dotenv().ok(); env::var("GANBARE_AUDIO_DIR")
        .unwrap_or_else(|_| "audio".into()) };

    pub static ref IMAGES_DIR : String = { dotenv::dotenv().ok(); env::var("GANBARE_IMAGES_DIR")
        .unwrap_or_else(|_| "images".into()) };

    pub static ref RUNTIME_PEPPER : Vec<u8> = { dotenv::dotenv().ok();
        let pepper = env::var("GANBARE_RUNTIME_PEPPER")
        .expect("Environmental variable GANBARE_RUNTIME_PEPPER must be set! (format: 256-bit random value encoded as base64)")
        .from_base64().expect("Environmental variable GANBARE_RUNTIME_PEPPER isn't valid Base64!");
        if pepper.len() != 32 { panic!("The value must be 256-bit, that is, 32 bytes long!") }; pepper
    };

}

pub fn db_connect() -> Result<PgConnection> {
    ganbare::db_connect(&*DATABASE_URL)
}


pub fn get_cookie(cookies : &Cookie) -> Option<&str> {
    for c in cookies.0.iter() {
        if c.name == "session_id" {
            return Some(c.value.as_ref());
        }
    };
    None
}

pub fn new_template_context() -> BTreeMap<String, String> {
    let mut ctx = BTreeMap::new();
    ctx.insert("title".to_string(), "akusento.ganba.re".to_string());
    ctx.insert("jquery_url".to_string(), JQUERY_URL.to_string());
    ctx.insert("font_stylesheet".to_string(), FONT_URL.to_string());
    ctx
}

pub fn get_user(conn : &PgConnection, req : &Request) -> Result<Option<(User, Session)>> {
    if let Some(session_id) = req.cookies().and_then(get_cookie) {
        ganbare::check_session(&conn, session_id)
            .map(|user_sess| Some(user_sess))
            .or_else(|e| match e.kind() {
                &ErrorKind::BadSessId => Ok(None),
                &ErrorKind::NoSuchSess => Ok(None),
                _ => Err(e),
            })
    } else {
        Ok(None)
    }
}

pub trait ResponseExt {
    fn refresh_cookie(self, &PgConnection, &Session, IpAddr) -> Self;
    fn expire_cookie(self) -> Self;
}

impl ResponseExt for Response {

    fn refresh_cookie(mut self, conn: &PgConnection, old_sess : &Session, ip: IpAddr) -> Self {
        let sess = ganbare::refresh_session(&conn, &old_sess, ip).expect("Session should already checked to be valid");
    
        let mut cookie = CookiePair::new("session_id".to_owned(), ganbare::sess_to_hex(&sess));
        cookie.path = Some("/".to_owned());
        cookie.domain = Some(SITE_DOMAIN.to_owned());
        cookie.expires = Some(time::now_utc() + time::Duration::weeks(2));
        self.set_cookie(SetCookie(vec![cookie]));
        self
    }
    
    fn expire_cookie(mut self) -> Self {
        let mut cookie = CookiePair::new("session_id".to_owned(), "".to_owned());
        cookie.path = Some("/".to_owned());
        cookie.domain = Some(SITE_DOMAIN.to_owned());
        cookie.expires = Some(time::at_utc(time::Timespec::new(0, 0)));
        self.set_cookie(SetCookie(vec![cookie]));
        self
    }
}


macro_rules! try_or {
    ($t:expr , else $e:expr ) => {  match $t { Some(x) => x, None => { $e } };  }
}

pub fn internal_error<T: std::fmt::Debug>(err: T) -> PencilError {
    error!("{:?}", err);
    PencilError::PenHTTPError(pencil::http_errors::HTTPError::InternalServerError)
}

pub fn bad_request<T: ToString>(err_msg: T) -> Response {
        let body = err_msg.to_string();
        let mut resp = pencil::Response::new(body);
        resp.status_code = 400;
        resp
}

pub trait ResultExt<T> {
    fn err_500(self) -> StdResult<T, PencilError>;
}

impl<T, E: std::fmt::Debug> ResultExt<T> for StdResult<T, E> {
    fn err_500(self) -> StdResult<T, PencilError> {
        self.map_err(|e| internal_error(e))
    }
}

pub trait CarrierInternal<T, E> where E: std::fmt::Debug {
    fn ok_or(self) -> std::result::Result<T, E>;
}

impl<T> CarrierInternal<T, ()> for Option<T> {
    fn ok_or(self) -> std::result::Result<T, ()> {
        match self {
            Some(a) => Ok(a),
            None => Err(()),
        }
    }
}
impl<T, E> CarrierInternal<T, E> for std::result::Result<T, E> where E: std::fmt::Debug {
    fn ok_or(self) -> std::result::Result<T, E> {
        match self {
            Ok(a) => Ok(a),
            Err(e) => Err(e),
        }
    }
}

macro_rules! err_400 {
    ($t:expr , $format_string:expr $(, $param:expr)* ) => { match CarrierInternal::ok_or($t) {
        Ok(a) => { a },
        Err(e) => {
            return Ok(bad_request(
                format!(concat!("<h1>HTTP 400 Bad Request {:?}: ", $format_string, "</h1>"), e $(, $param)*)
            ))
        },
    } }
}

#[cfg(debug_assertions)]
macro_rules! include_templates(
    ($app:ident, $temp_dir:expr, $($file:expr),*) => { {
        $app.template_folder = $temp_dir.to_string();
        $(
            $app.register_template($file);
        )*
        info!("Templates loaded.");
    } }
);

#[cfg(not(debug_assertions))]
macro_rules! include_templates(
    ($app:ident, $temp_dir:expr, $($file:expr),*) => { {
        let mut reg = $app.handlebars_registry.write().expect("This is supposed to fail fast and hard.");
        $(
        reg.register_template_string($file, include_str!(concat!(env!("PWD"), "/", $temp_dir, "/", $file)).to_string())
        .expect("This is supposed to fail fast and hard.");
        )*
    } }
);


pub fn auth_user(req: &mut Request, required_group: &str)
    -> StdResult<(PgConnection, User, Session), PencilError>
{
    match try_auth_user(req)? {
        Some((conn, user, sess)) => {
            if ganbare::check_user_group(&conn, &user, required_group).err_500()? {
                Ok((conn, user, sess))
            } else {
                Err(abort(401).unwrap_err()) // User doesn't belong in the required groups
            }
        },
        None => {
            Err(abort(401).unwrap_err()) // User isn't logged in
        },
    }

}

pub fn try_auth_user(req: &mut Request)
    -> StdResult<Option<(PgConnection, User, Session)>, PencilError> {

    let conn = db_connect().err_500()?;

    if let Some((user, sess)) = get_user(&conn, req).err_500()?
    { // User is logged in

        Ok(Some((conn, user, sess)))

    } else { // Not logged in
        Ok(None)
    }

}

pub fn check_env_vars() { &*DATABASE_URL; &*EMAIL_SERVER; &*SITE_DOMAIN; }

pub fn do_login(email : &str, plaintext_pw : &str, ip : IpAddr) -> StdResult<Option<(User, Session)>, PencilError> {
    let conn = db_connect().err_500()?;
    let user = try_or!(ganbare::auth_user(&conn, email, plaintext_pw, &*RUNTIME_PEPPER).err_500()?,
            else return Ok(None));

    let sess = ganbare::start_session(&conn, &user, ip).err_500()?;

    Ok(Some((user, sess)))
}


macro_rules! parse {
    ($expression:expr) => {$expression.map(String::to_string).ok_or(ErrorKind::FormParseError.to_err())?;}
}
