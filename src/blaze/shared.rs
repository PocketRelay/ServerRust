use std::fmt::Debug;

use crate::blaze::session::SessionData;
use crate::database::entities::PlayerModel;
use blaze_pk::{
    packet, tag_empty_blob, tag_empty_str, tag_group_end, tag_group_start, tag_list_start,
    tag_map_start, tag_str, tag_u16, tag_u32, tag_u64, tag_u8, tag_value, tag_var_int_list_empty,
    tag_zero, Codec, CodecResult, Reader, Tag, TdfOptional, ValueType,
};

pub struct SetSessionDetails<'a> {
    pub session: &'a SessionData,
}

//noinspection SpellCheckingInspection
impl Codec for SetSessionDetails<'_> {
    fn encode(&self, output: &mut Vec<u8>) {
        self.session.encode(output);
        tag_u32(output, "USID", self.session.id_safe());
    }
}

pub struct SessionDetails<'a> {
    pub session: &'a SessionData,
    pub player: &'a PlayerModel,
}

//noinspection SpellCheckingInspection
impl Codec for SessionData {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_group_start(output, "DATA");
        tag_value(output, "ADDR", &self.net.get_groups());
        tag_str(output, "BPS", "ea-sjc");
        tag_empty_str(output, "CTY");
        tag_var_int_list_empty(output, "CVAR");
        {
            tag_map_start(output, "DMAP", ValueType::VarInt, ValueType::VarInt, 1);
            0x70001.encode(output);
            0x409a.encode(output);
        }
        tag_u16(output, "HWFG", self.hardware_flag);
        {
            tag_list_start(output, "PSLM", ValueType::VarInt, 1);
            0xfff0fff.encode(output);
        }
        tag_value(output, "QDAT", &self.net.ext);
        tag_u8(output, "UATT", 0);
        if let Some(session_game) = &self.game {
            tag_list_start(output, "ULST", ValueType::Triple, 1);
            (4, 1, session_game.game.id).encode(output);
        }
        tag_group_end(output);
    }

    fn value_type() -> ValueType {
        ValueType::Group
    }
}

//noinspection SpellCheckingInspection
impl Codec for SessionDetails<'_> {
    fn encode(&self, output: &mut Vec<u8>) {
        self.session.encode(output);
        {
            tag_group_start(output, "USER");
            tag_u32(output, "AID", self.player.id);
            tag_u32(output, "ALOC", self.session.location);
            tag_empty_blob(output, "EXBB");
            tag_u8(output, "EXID", 0);
            tag_u32(output, "ID", self.player.id);
            tag_str(output, "NAME", &self.player.display_name);
            tag_group_end(output);
        }
    }
}

packet! {
    struct UpdateExtDataAttr {
        FLGS flags: u8,
        ID id: u32
    }
}

/// Structure for storing extended network data
#[derive(Debug, Copy, Clone, Default)]
pub struct NetExt {
    pub dbps: u16,
    pub natt: u8,
    pub ubps: u16,
}

//noinspection SpellCheckingInspection
impl Codec for NetExt {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_u16(output, "DBPS", self.dbps);
        tag_u8(output, "NATT", self.natt);
        tag_u16(output, "UBPS", self.ubps);
        output.push(0)
    }

    fn decode(reader: &mut Reader) -> CodecResult<Self> {
        let dbps = Tag::expect(reader, "DBPS")?;
        let natt = Tag::expect(reader, "NATT")?;
        let ubps = Tag::expect(reader, "UBPS")?;
        reader.take_one()?;
        Ok(Self { dbps, natt, ubps })
    }

    fn value_type() -> ValueType {
        ValueType::Group
    }
}

/// Type alias for ports which are always u16
pub type Port = u16;

#[derive(Debug, Default)]
pub struct NetData {
    pub groups: NetGroups,
    pub ext: NetExt,
    pub is_unset: bool,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct NetGroups {
    pub internal: NetGroup,
    pub external: NetGroup,
}

//noinspection SpellCheckingInspection
impl Codec for NetGroups {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_group_start(output, "EXIP");
        self.external.encode(output);

        tag_group_start(output, "INIP");
        self.internal.encode(output);

        tag_group_end(output);
    }

    fn decode(reader: &mut Reader) -> CodecResult<Self> {
        let external = Tag::expect(reader, "EXIP")?;
        let internal = Tag::expect(reader, "INIP")?;
        reader.take_one()?;
        Ok(Self { external, internal })
    }

    fn value_type() -> ValueType {
        ValueType::Group
    }
}

impl NetData {
    pub fn get_groups(&self) -> TdfOptional<NetGroups> {
        if self.is_unset {
            TdfOptional::None
        } else {
            TdfOptional::Some(0x2, (String::from("VALU"), self.groups))
        }
    }
}

/// Structure for a networking group which consists of a
/// networking address and port value
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub struct NetGroup(pub NetAddress, pub Port);

impl Codec for NetGroup {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_u32(output, "IP", self.0 .0);
        tag_u16(output, "PORT", self.1);
        tag_group_end(output);
    }

    fn decode(reader: &mut Reader) -> CodecResult<Self> {
        let ip = Tag::expect(reader, "IP")?;
        let port = Tag::expect(reader, "PORT")?;
        reader.take_one()?;
        Ok(Self(NetAddress(ip), port))
    }

    fn value_type() -> ValueType {
        ValueType::Group
    }
}

/// Structure for wrapping a Blaze networking address
#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct NetAddress(pub u32);

impl Debug for NetAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_invalid() {
            f.write_str("INVALID_ADDR")
        } else {
            let value = self.to_ipv4();
            f.write_str(&value)
        }
    }
}

impl NetAddress {
    pub fn is_invalid(&self) -> bool {
        self.0 == 0
    }

    /// Converts the provided IPv4 string into a NetAddress
    pub fn from_ipv4(value: &str) -> NetAddress {
        let parts = value
            .split(".")
            .filter_map(|value| value.parse::<u32>().ok())
            .collect::<Vec<u32>>();
        if parts.len() < 4 {
            return NetAddress(0);
        }
        let value = parts[0] << 24 | parts[1] << 16 | parts[2] << 8 | parts[3];
        NetAddress(value)
    }

    /// Converts the value stored in this NetAddress to an IPv4 string
    pub fn to_ipv4(&self) -> String {
        let a = ((self.0 >> 24) & 0xFF) as u8;
        let b = ((self.0 >> 16) & 0xFF) as u8;
        let c = ((self.0 >> 8) & 0xFF) as u8;
        let d = (self.0 & 0xFF) as u8;
        format!("{a}.{b}.{c}.{d}")
    }
}

#[inline]
fn encode_persona(player: &PlayerModel, output: &mut Vec<u8>) {
    tag_str(output, "DSNM", &player.display_name);
    tag_zero(output, "LAST");
    tag_u32(output, "PID", player.id);
    tag_zero(output, "STAS");
    tag_zero(output, "XREF");
    tag_zero(output, "XTYP");
    tag_group_end(output);
}

#[derive(Debug)]
pub struct Sess<'a> {
    pub player: &'a PlayerModel,
    pub session_token: String,
}

impl Codec for Sess<'_> {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_u32(output, "BUID", self.player.id);
        tag_zero(output, "FRST");
        tag_str(output, "KEY", &self.session_token);
        tag_zero(output, "LLOG");
        tag_str(output, "MAIL", &self.player.email);
        tag_group_start(output, "PDTL");
        encode_persona(&self.player, output);
        tag_u32(output, "UID", self.player.id);
    }
}

/// Complex authentication result structure is manually encoded because it
/// has complex nesting and output can vary based on inputs provided
#[derive(Debug)]
pub struct AuthRes<'a> {
    pub sess: Sess<'a>,
    pub silent: bool,
}

impl Codec for AuthRes<'_> {
    fn encode(&self, output: &mut Vec<u8>) {
        let silent = self.silent;
        if silent {
            tag_zero(output, "AGUP");
        }

        tag_empty_str(output, "LDHT");
        tag_zero(output, "NTOS");
        tag_str(output, "PCTK", &self.sess.session_token);

        if silent {
            tag_empty_str(output, "PRIV");
            tag_group_start(output, "SESS");
            self.sess.encode(output);
            tag_group_end(output);
        } else {
            tag_list_start(output, "PLST", ValueType::Group, 1);
            encode_persona(&self.sess.player, output);
            tag_empty_str(output, "PRIV");
            tag_str(output, "SKEY", &self.sess.session_token);
        }
        tag_zero(output, "SPAM");
        tag_empty_str(output, "THST");
        tag_empty_str(output, "TSUI");
        tag_empty_str(output, "TURI");
        if !silent {
            tag_u32(output, "UID", self.sess.player.id);
        }
    }
}

//noinspection SpellCheckingInspection
#[derive(Debug)]
pub struct Entitlement<'a> {
    name: &'a str,
    id: u64,
    pjid: &'a str,
    prca: u8,
    prid: &'a str,
    tag: &'a str,
    ty: u8,
}

impl<'a> Entitlement<'a> {
    const PC_TAG: &'a str = "ME3PCOffers";
    const GEN_TAG: &'a str = "ME3GenOffers";

    pub fn new_pc(id: u64, pjid: &'a str, prca: u8, prid: &'a str, tag: &'a str, ty: u8) -> Self {
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

    pub fn new_gen(id: u64, pjid: &'a str, prca: u8, prid: &'a str, tag: &'a str, ty: u8) -> Self {
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

impl Codec for Entitlement<'_> {
    //noinspection SpellCheckingInspection
    fn encode(&self, output: &mut Vec<u8>) {
        tag_empty_str(output, "DEVI");
        tag_str(output, "GDAY", "2012-12-15T16:15Z");
        tag_str(output, "GNAM", self.name);
        tag_u64(output, "ID", self.id);
        tag_u8(output, "ISCO", 0);
        tag_u8(output, "PID", 0);
        tag_str(output, "PJID", self.pjid);
        tag_u8(output, "PRCA", self.prca);
        tag_str(output, "PRID", self.prid);
        tag_u8(output, "STAT", 1);
        tag_u8(output, "STRC", 0);
        tag_str(output, "TAG", self.tag);
        tag_empty_str(output, "TDAY");
        tag_u8(output, "TTYPE", self.ty);
        tag_u8(output, "UCNT", 0);
        tag_u8(output, "VER", 0);
        tag_group_end(output);
    }

    fn value_type() -> ValueType {
        ValueType::Group
    }
}

#[derive(Debug)]
pub struct LegalDocsInfo;

impl Codec for LegalDocsInfo {
    //noinspection SpellCheckingInspection
    fn encode(&self, output: &mut Vec<u8>) {
        tag_zero(output, "EAMC");
        tag_empty_str(output, "LHST");
        tag_zero(output, "PMC");
        tag_empty_str(output, "PPUI");
        tag_empty_str(output, "TSUI");
    }
}

#[derive(Debug)]
pub struct TermsContent<'a, 'b> {
    pub path: &'a str,
    pub col: u16,
    pub content: &'b str,
}

impl Codec for TermsContent<'_, '_> {
    //noinspection SpellCheckingInspection
    fn encode(&self, output: &mut Vec<u8>) {
        tag_str(output, "LDVC", self.path);
        tag_u16(output, "TCOL", self.col);
        tag_str(output, "TCOT", self.content);
    }
}

#[derive(Debug)]
pub struct TelemetryRes {
    pub(crate) address: String,
    pub(crate) session_id: u32,
}

impl Codec for TelemetryRes {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_str(output, "ADRS", &self.address);
        tag_zero(output, "ANON");
        tag_str(output, "DISA", "AD,AF,AG,AI,AL,AM,AN,AO,AQ,AR,AS,AW,AX,AZ,BA,BB,BD,BF,BH,BI,BJ,BM,BN,BO,BR,BS,BT,BV,BW,BY,BZ,CC,CD,CF,CG,CI,CK,CL,CM,CN,CO,CR,CU,CV,CX,DJ,DM,DO,DZ,EC,EG,EH,ER,ET,FJ,FK,FM,FO,GA,GD,GE,GF,GG,GH,GI,GL,GM,GN,GP,GQ,GS,GT,GU,GW,GY,HM,HN,HT,ID,IL,IM,IN,IO,IQ,IR,IS,JE,JM,JO,KE,KG,KH,KI,KM,KN,KP,KR,KW,KY,KZ,LA,LB,LC,LI,LK,LR,LS,LY,MA,MC,MD,ME,MG,MH,ML,MM,MN,MO,MP,MQ,MR,MS,MU,MV,MW,MY,MZ,NA,NC,NE,NF,NG,NI,NP,NR,NU,OM,PA,PE,PF,PG,PH,PK,PM,PN,PS,PW,PY,QA,RE,RS,RW,SA,SB,SC,SD,SG,SH,SJ,SL,SM,SN,SO,SR,ST,SV,SY,SZ,TC,TD,TF,TG,TH,TJ,TK,TL,TM,TN,TO,TT,TV,TZ,UA,UG,UM,UY,UZ,VA,VC,VE,VG,VN,VU,WF,WS,YE,YT,ZM,ZW,ZZ");
        tag_str(output, "FILT", "-UION/****");
        tag_u32(output, "LOC", 0x656e5553);
        tag_str(output, "NOOK", "US,CA,MX");
        tag_u16(output, "PORT", 9988);
        tag_u16(output, "SDLY", 15000);
        tag_str(output, "SESS", "Evi8itOCVpD");
        tag_str(output, "SKEY", &self.session_id.to_string());
        tag_u8(output, "SPCT", 75);
        tag_empty_str(output, "STIM");
    }
}

packet! {
    struct SessionStateChange {
        GID gid: u32,
        PID pid: u32,
        STAT state: u8,
    }
}

packet! {
    struct NotifyJoinComplete {
        GID gid: u32,
        PID pid: u32,
    }
}

packet! {
    struct NotifyAdminListChange {
        ALST alst: u32,
        GID gid: u32,
        OPER oper: u8,
        UID uid: u32,
    }
}
