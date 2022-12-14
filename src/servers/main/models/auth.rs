use crate::utils::types::PlayerID;
use blaze_pk::{
    codec::{Decodable, Encodable},
    error::{DecodeError, DecodeResult},
    reader::TdfReader,
    tag::TdfType,
    value_type,
    writer::TdfWriter,
};
use database::Player;
use std::borrow::Cow;

/// Different possible authentication request types.
pub enum AuthRequest {
    /// Silent token based authentication with player ID
    Silent {
        /// The authentication token previously provided to the client
        /// on a previous successful authentication attempt
        token: String,
        /// The ID of the player that the provided token is for
        player_id: PlayerID,
    },
    /// Login through login prompt menu with email and password
    Login {
        /// The email addresss of the account to login with
        email: String,
        /// The plain text password of the account to login to
        password: String,
    },
    /// AUthentication through origin token
    Origin {
        /// The token generated by Origin
        token: String,
    },
}

impl AuthRequest {
    /// Function for determining whether the type of auth request
    /// is one that happens silently in the background without user
    /// input.
    pub fn is_silent(&self) -> bool {
        match self {
            Self::Silent { .. } | Self::Origin { .. } => true,
            Self::Login { .. } => false,
        }
    }
}

impl Decodable for AuthRequest {
    fn decode(reader: &mut TdfReader) -> DecodeResult<Self> {
        let ty: u8 = {
            let start: usize = reader.cursor;
            let ty: u8 = reader.tag("TYPE")?;
            reader.cursor = start;
            ty
        };

        match ty {
            0 => {
                let email: String = reader.tag("MAIL")?;
                let password: String = reader.tag("PASS")?;
                Ok(Self::Login { email, password })
            }
            1 => {
                let token: String = reader.tag("AUTH")?;
                Ok(Self::Origin { token })
            }
            2 => {
                let token: String = reader.tag("AUTH")?;
                let player_id: u32 = reader.tag("PID")?;
                Ok(Self::Silent { token, player_id })
            }
            _ => Err(DecodeError::UnknownType { ty }),
        }
    }
}

/// Encodes a mock persona from the provided player using its
/// display name and ID as the values
///
/// `writer`       The Tdf writer to use for writing the values
/// `id`           The id of the player to write for
/// `display_name` The display name of the player
fn encode_persona(writer: &mut TdfWriter, id: PlayerID, display_name: &str) {
    writer.tag_str(b"DSNM", display_name);
    writer.tag_zero(b"LAST");
    writer.tag_u32(b"PID", id);
    writer.tag_zero(b"STAS");
    writer.tag_zero(b"XREF");
    writer.tag_zero(b"XTYP");
    writer.tag_group_end();
}

/// Structure for the response to an authentication request.
pub struct AuthResponse<'a> {
    /// The authenticated player
    pub player: &'a Player,
    /// The session token for the completed authentication
    pub session_token: String,
    /// Whether the authentication proccess was silent
    pub silent: bool,
}

impl Encodable for AuthResponse<'_> {
    fn encode(&self, writer: &mut TdfWriter) {
        if self.silent {
            writer.tag_zero(b"AGUP");
        }
        writer.tag_str_empty(b"LDHT");
        writer.tag_zero(b"NTOS");
        writer.tag_str(b"PCTK", &self.session_token); // PC Authentication Token
        if self.silent {
            writer.tag_str_empty(b"PRIV");
            {
                writer.tag_group(b"SESS");
                writer.tag_u32(b"BUID", self.player.id);
                writer.tag_zero(b"FRST");
                writer.tag_str(b"KEY", &self.session_token); // Session Token
                writer.tag_zero(b"LLOG");
                writer.tag_str(b"MAIL", &self.player.email); // Player Email
                {
                    writer.tag_group(b"PDTL");
                    encode_persona(writer, self.player.id, &self.player.display_name);
                    // Persona Details
                }
                writer.tag_u32(b"UID", self.player.id);
                writer.tag_group_end();
            }
        } else {
            writer.tag_list_start(b"PLST", TdfType::Group, 1);
            encode_persona(writer, self.player.id, &self.player.display_name);
            writer.tag_str_empty(b"PRIV");
            writer.tag_str(b"SKEY", &self.session_token);
        }
        writer.tag_zero(b"SPAM");
        writer.tag_str_empty(b"THST");
        writer.tag_str_empty(b"TSUI");
        writer.tag_str_empty(b"TURI");
        if !self.silent {
            writer.tag_u32(b"UID", self.player.id);
        }
    }
}

/// Structure for request to create a new account with
/// the provided email and password
pub struct CreateAccountRequest {
    /// The email address of the account to create
    pub email: String,
    /// The password of the account to create
    pub password: String,
}

impl Decodable for CreateAccountRequest {
    fn decode(reader: &mut TdfReader) -> DecodeResult<Self> {
        let email: String = reader.tag("MAIL")?;
        let password: String = reader.tag("PASS")?;
        Ok(Self { email, password })
    }
}

/// Structure for the persona response which contains details
/// about the current persona. Which in this case is just the
/// player details
pub struct PersonaResponse<'a> {
    /// The player
    pub player: &'a Player,

    /// The players current session token
    pub session_token: &'a String,
}

impl Encodable for PersonaResponse<'_> {
    fn encode(&self, writer: &mut TdfWriter) {
        writer.tag_u32(b"BUID", self.player.id);
        writer.tag_zero(b"FRST");
        writer.tag_str(b"KEY", self.session_token);
        writer.tag_zero(b"LLOG");
        writer.tag_str(b"MAIL", &self.player.email);

        writer.tag_group(b"PDTL");
        encode_persona(writer, self.player.id, &self.player.display_name);
        writer.tag_u32(b"UID", self.player.id);
    }
}

/// Request for listing entitlements
pub struct ListEntitlementsRequest {
    /// The entitlements tag
    pub tag: String,
}

impl Decodable for ListEntitlementsRequest {
    fn decode(reader: &mut TdfReader) -> DecodeResult<Self> {
        let tag: String = reader.tag("ETAG")?;
        Ok(Self { tag })
    }
}

/// Response of an entitlements list
pub struct ListEntitlementsResponse {
    pub list: Vec<Entitlement>,
}

impl Encodable for ListEntitlementsResponse {
    fn encode(&self, writer: &mut TdfWriter) {
        writer.tag_value(b"NLST", &self.list);
    }
}

//noinspection SpellCheckingInspection
pub struct Entitlement {
    pub name: &'static str,
    pub id: u64,
    pub pjid: &'static str,
    pub prca: u8,
    pub prid: &'static str,
    pub tag: &'static str,
    pub ty: u8,
}

impl Entitlement {
    pub const PC_TAG: &'static str = "ME3PCOffers";
    pub const GEN_TAG: &'static str = "ME3GenOffers";

    pub fn new_pc(
        id: u64,
        pjid: &'static str,
        prca: u8,
        prid: &'static str,
        tag: &'static str,
        ty: u8,
    ) -> Self {
        Self {
            name: Self::PC_TAG,
            id,
            pjid,
            prca,
            prid,
            tag,
            ty,
        }
    }

    pub fn new_gen(
        id: u64,
        pjid: &'static str,
        prca: u8,
        prid: &'static str,
        tag: &'static str,
        ty: u8,
    ) -> Self {
        Self {
            name: Self::GEN_TAG,
            id,
            pjid,
            prca,
            prid,
            tag,
            ty,
        }
    }
}

impl Encodable for Entitlement {
    fn encode(&self, writer: &mut TdfWriter) {
        writer.tag_str_empty(b"DEVI");
        writer.tag_str(b"GDAY", "2012-12-15T16:15Z");
        writer.tag_str(b"GNAM", self.name);
        writer.tag_u64(b"ID", self.id);
        writer.tag_u8(b"ISCO", 0);
        writer.tag_u8(b"PID", 0);
        writer.tag_str(b"PJID", self.pjid);
        writer.tag_u8(b"PRCA", self.prca);
        writer.tag_str(b"PRID", self.prid);
        writer.tag_u8(b"STAT", 1);
        writer.tag_u8(b"STRC", 0);
        writer.tag_str(b"TAG", self.tag);
        writer.tag_str_empty(b"TDAY");
        writer.tag_u8(b"TTYPE", self.ty);
        writer.tag_u8(b"UCNT", 0);
        writer.tag_u8(b"VER", 0);
        writer.tag_group_end();
    }
}

value_type!(Entitlement, TdfType::Group);

/// Structure for a request to send a forgot password email. Currently
/// only logs that a reset was requested and doesn't actually send
/// an email.
pub struct ForgotPasswordRequest {
    /// The email of the account that needs a password reset
    pub email: String,
}

impl Decodable for ForgotPasswordRequest {
    fn decode(reader: &mut TdfReader) -> DecodeResult<Self> {
        let email: String = reader.tag("MAIL")?;
        Ok(Self { email })
    }
}

/// Dummy structure for the LegalDocsInfo response. None of the
/// values in this struct ever change.
pub struct LegalDocsInfo;

impl Encodable for LegalDocsInfo {
    fn encode(&self, writer: &mut TdfWriter) {
        writer.tag_zero(b"EAMC");
        writer.tag_str_empty(b"LHST");
        writer.tag_zero(b"PMC");
        writer.tag_str_empty(b"PPUI");
        writer.tag_str_empty(b"TSUI");
    }
}

/// Structure for legal content responses such as the Privacy Policy
/// and the terms and condition.
pub struct LegalContent {
    /// The url path to the legal content (Prefix this value with https://tos.ea.com/legalapp/ to get the url)
    pub path: &'static str,
    /// The actual HTML content of the legal document
    pub content: Cow<'static, str>,
    /// Unknown value
    pub col: u16,
}

impl Encodable for LegalContent {
    fn encode(&self, writer: &mut TdfWriter) {
        writer.tag_str(b"LDVC", self.path);
        writer.tag_u16(b"TCOL", self.col);
        writer.tag_str(b"TCOT", &self.content);
    }
}

/// Response to the client requesting a shared token
pub struct GetTokenResponse {
    /// The generated shared token
    pub token: String,
}

impl Encodable for GetTokenResponse {
    fn encode(&self, writer: &mut TdfWriter) {
        writer.tag_str(b"AUTH", &self.token)
    }
}
