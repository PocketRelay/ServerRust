use blaze_pk::{Blob, Codec, CodecResult, decode_field, group, packet, Reader, Tag, tag_empty_str, tag_group_end, tag_group_start, tag_list_start, tag_str, tag_u16, tag_u32, tag_u64, tag_u8, tag_zero, TdfMap, TdfOptional, ValueType, VarIntList};
use crate::blaze::SessionData;
use crate::database::entities::PlayerModel;


packet! {
    struct SessionDetails {
        DATA data: SessionDataCodec,
        USER user: SessionUser
    }
}

packet! {
    struct UpdateExtDataAttr {
        FLGS flags: u8,
        ID id: u32
    }
}

packet! {
    struct SessionUser {
        AID aid: u32,
        ALOC location: u32,
        EXBB exbb: Blob,
        EXID exid: u8,
        ID id: u32,
        NAME name: String
    }
}

group! {
    struct SessionDataCodec {
        ADDR addr: TdfOptional<NetGroups>,
        BPS bps: &'static str,
        CTY cty: &'static str,
        CVAR cvar: VarIntList<u16>,
        DMAP dmap: TdfMap<u32, u32>,
        HWFG hardware_flag: u16,
        PSLM pslm: Vec<u32>,
        QDAT net_ext: NetExt,
        UATT uatt: u8,
        ULST ulst: Vec<(u8, u8, u32)>
    }
}

/// Structure for storing extended network data
#[derive(Debug, Copy, Clone, Default)]
pub struct NetExt {
    pub dbps: u16,
    pub natt: u8,
    pub ubps: u16,
}

impl Codec for NetExt {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_u16(output, "DBPS", self.dbps);
        tag_u8(output, "NATT", self.natt);
        tag_u16(output, "UBPS", self.ubps);
        output.push(0)
    }

    fn decode(reader: &mut Reader) -> CodecResult<Self> {
        decode_field!(reader, DBPS, dbps, u16);
        decode_field!(reader, NATT, natt, u8);
        decode_field!(reader, UBPS, ubps, u16);
        reader.take_one()?;
        Ok(Self { dbps, natt, ubps })
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
        tag_group_end(output);

        tag_group_start(output, "INIP");
        self.internal.encode(output);
        tag_group_end(output);

        output.push(0)
    }

    fn decode(reader: &mut Reader) -> CodecResult<Self> {
        Tag::expect_tag("EXIP", &ValueType::Group, reader)?;
        let external = NetGroup::decode(reader)?;
        reader.take_one()?;

        Tag::expect_tag("INIP", &ValueType::Group, reader)?;
        let internal = NetGroup::decode(reader)?;
        reader.take_one()?;

        reader.take_one()?;
        Ok(Self {
            external,
            internal,
        })
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
        tag_u32(output, "IP", self.0.0);
        tag_u16(output, "PORT", self.1);
    }

    fn decode(reader: &mut Reader) -> CodecResult<Self> {
        Tag::expect_tag("IP", &ValueType::VarInt, reader)?;
        let ip = u32::decode(reader)?;
        Tag::expect_tag("PORT", &ValueType::VarInt, reader)?;
        let port = u16::decode(reader)?;
        Ok(Self(NetAddress(ip), port))
    }

    fn value_type() -> ValueType {
        ValueType::Group
    }
}

/// Structure for wrapping a Blaze networking address
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq)]
pub struct NetAddress(pub u32);

impl NetAddress {
    /// Converts the provided IPv4 string into a NetAddress
    pub fn from_ipv4(value: &str) -> NetAddress {
        let parts = value.split(".")
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
pub struct Sess<'a, 'b> {
    pub session_data: &'a SessionData,
    pub player: &'b PlayerModel,
    pub session_token: String,
}

impl Codec for Sess<'_, '_> {
    fn encode(&self, output: &mut Vec<u8>) {
        tag_group_start(output, "SESS");
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
pub struct AuthRes<'a, 'b> {
    pub sess: Sess<'a, 'b>,
    pub silent: bool,
}

impl Codec for AuthRes<'_, '_> {
    fn encode(&self, output: &mut Vec<u8>) {
        let silent = self.silent;
        if silent {
            tag_zero(output, "AGUP");
        }

        tag_empty_str(output, "LDHT");
        tag_zero(output, "NTOS");
        tag_str(output, "PCTK", &self.session_token);

        if silent {
            tag_empty_str(output, "PRIV");
            tag_group_start(output, "SESS");
            self.sess.encode(output);
            tag_group_end(output);
        } else {
            tag_list_start(output, "PLST", ValueType::Group, 1);
            encode_persona(&self.player, output);
            tag_empty_str(output, "PRIV");
            tag_str(output, "SKEY", &self.sess.session_token);
        }
        tag_zero(output, "SPAM");
        tag_empty_str(output, "THST");
        tag_empty_str(output, "TSUI");
        tag_empty_str(output, "TURI");
        if !silent {
            tag_u32(output, "UID", self.player.id);
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
    const DLC_TY: u8 = 5;
    const EXT_TY: u8 = 1;

    const PC_TAG: &'a str = "ME3PCOffers";
    const GEN_TAG: &'a str = "ME3GenOffers";

    pub fn new_pc(
        id: u64,
        pjid: &'a str,
        prca: u8,
        prid: &'a str,
        tag: &'a str,
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
        pjid: &'a str,
        prca: u8,
        prid: &'a str,
        tag: &'a str,
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