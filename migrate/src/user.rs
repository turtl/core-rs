//! Holds some of the code for the old user model, mainly pertaining to key and
//! auth tag generation.

use ::crypto::{self, Key};
use ::error::{MResult, MError};

/// Generate a user's key given some variables or something
fn generate_key(username: &String, password: &String, version: u16, iterations: usize) -> MResult<Key> {
    let key: Key = match version {
        0 => {
            let mut salt = String::from(&username[..]);
            salt.push_str(":a_pinch_of_salt");  // and laughter too
            crypto::gen_key(crypto::Hasher::SHA1, password.as_ref(), salt.as_bytes(), 400)?
        },
        1 => {
            let salt = crypto::to_hex(&crypto::sha256(username.as_bytes())?)?;
            crypto::gen_key(crypto::Hasher::SHA256, password.as_ref(), &salt.as_bytes(), iterations)?
        },
        _ => return Err(MError::NotImplemented),
    };
    Ok(key)
}

/// Generate a user's auth token given some variables or something
pub fn generate_auth(username: &String, password: &String, version: u16) -> MResult<(Key, String)> {
    info!("user::generate_auth() -- generating v{} auth", version);
    let key_auth = match version {
        0 => {
            let key = generate_key(&username, &password, version, 0)?;
            let iv_str = String::from(&username[..]) + "4c281987249be78a";
            let mut iv = Vec::from(iv_str.as_bytes());
            iv.truncate(16);
            let mut user_record = crypto::to_hex(&crypto::sha256(&password.as_bytes())?)?;
            user_record.push_str(":");
            user_record.push_str(&username[..]);
            let auth = crypto::encrypt_v0(&key, &iv, &user_record)?;
            (key, auth)
        },
        1 => {
            let key = generate_key(&username, &password, version, 100000)?;
            let concat = String::from(&password[..]) + &username;
            let iv_bytes = crypto::sha256(concat.as_bytes())?;
            let iv_str = crypto::to_hex(&iv_bytes)?;
            let iv = Vec::from(&iv_str.as_bytes()[0..16]);
            let pw_hash = crypto::to_hex(&crypto::sha256(&password.as_bytes())?)?;
            let un_hash = crypto::to_hex(&crypto::sha256(&username.as_bytes())?)?;
            let mut user_record = String::from(&pw_hash[..]);
            user_record.push_str(":");
            user_record.push_str(&un_hash[..]);
            let utf8_byte: u8 = u8::from_str_radix(&user_record[18..20], 16)?;
            // have to do a stupid conversion here because of stupidity in the
            // original turtl code. luckily there will be a v2 gen_auth...
            let utf8_random: u8 = (((utf8_byte as f64) / 256.0) * 128.0).floor() as u8;
            let op = crypto::CryptoOp::new_with_iv_utf8("aes", "gcm", iv, utf8_random)?;
            let auth_bin = crypto::encrypt(&key, Vec::from(user_record.as_bytes()), op)?;
            let auth = crypto::to_base64(&auth_bin)?;
            (key, auth)

        },
        _ => return Err(MError::NotImplemented),
    };
    Ok(key_auth)
}

// -----------------------------------------------------------------------------
// login debug crap
// -----------------------------------------------------------------------------

fn log_val(name: &str, value: &String) -> String {
    format!("{}: {}", name, value)
}

fn log_secret_v(name: &str, vec: &[u8]) -> String {
    let key: Vec<u8> = vec![236, 249, 58, 218, 97, 168, 59, 164, 102, 126, 209, 175, 181, 5, 175, 210];
    let hmac = match crypto::hmac(crypto::Hasher::SHA1, key.as_slice(), vec) {
        Ok(x) => crypto::to_hex(&x).unwrap(),
        Err(e) => format!("error generating hmac: {}", e),
    };
    log_val(name, &hmac)
}

fn log_secret(name: &str, secret: &String) -> String {
    log_secret_v(name, secret.as_bytes())
}

/// Generate a user's key given some variables or something
fn generate_key_debug(username: &String, password: &String, version: u16, iterations: usize) -> MResult<(Key, Vec<String>)> {
    let mut log: Vec<String> = Vec::new();
    log.push(log_val("username(raw)", username));
    log.push(log_secret("username", username));
    log.push(log_secret("password", password));
    let key: Key = match version {
        0 => {
            let mut salt = String::from(&username[..]);
            salt.push_str(":a_pinch_of_salt");  // and laughter too
            log.push(log_val("salt", &salt));
            crypto::gen_key(crypto::Hasher::SHA1, password.as_ref(), salt.as_bytes(), 400)?
        },
        1 => {
            let salt = crypto::to_hex(&crypto::sha256(username.as_bytes())?)?;
            log.push(log_val("salt", &salt));
            crypto::gen_key(crypto::Hasher::SHA256, password.as_ref(), &salt.as_bytes(), iterations)?
        },
        _ => return Err(MError::NotImplemented),
    };
    log.push(log_secret_v("key", &key.data().as_slice()));
    Ok((key, log))
}

/// Generate a user's auth token given some variables or something
pub fn generate_auth_debug(username: &String, password: &String, version: u16) -> MResult<((Key, String), Vec<String>)> {
    let mut log: Vec<String> = Vec::new();
    info!("user::generate_auth() -- generating v{} auth", version);
    let key_auth = match version {
        0 => {
            let (key, mut keylog) = generate_key_debug(&username, &password, version, 0)?;
            log.append(&mut keylog);
            let iv_str = String::from(&username[..]) + "4c281987249be78a";
            log.push(log_val("iv", &iv_str));
            let mut iv = Vec::from(iv_str.as_bytes());
            iv.truncate(16);
            log.push(log_secret_v("iv", iv.as_slice()));
            let mut user_record = crypto::to_hex(&crypto::sha256(&password.as_bytes())?)?;
            user_record.push_str(":");
            user_record.push_str(&username[..]);
            log.push(log_secret("rec", &user_record));
            let auth = crypto::encrypt_v0(&key, &iv, &user_record)?;
            log.push(log_secret("auth", &auth));
            (key, auth)
        },
        1 => {
            let (key, mut keylog) = generate_key_debug(&username, &password, version, 100000)?;
            log.append(&mut keylog);
            let concat = String::from(&password[..]) + &username;
            log.push(log_secret("concat", &concat));
            let iv_bytes = crypto::sha256(concat.as_bytes())?;
            let iv_str = crypto::to_hex(&iv_bytes)?;
            log.push(log_secret("iv1", &iv_str));
            let iv = Vec::from(&iv_str.as_bytes()[0..16]);
            log.push(log_secret_v("iv2", &iv));
            let pw_hash = crypto::to_hex(&crypto::sha256(&password.as_bytes())?)?;
            let un_hash = crypto::to_hex(&crypto::sha256(&username.as_bytes())?)?;
            log.push(log_secret("pw_hash", &pw_hash));
            log.push(log_secret("un_hash", &un_hash));
            let mut user_record = String::from(&pw_hash[..]);
            user_record.push_str(":");
            user_record.push_str(&un_hash[..]);
            log.push(log_secret("rec", &user_record));
            let utf8_byte: u8 = u8::from_str_radix(&user_record[18..20], 16)?;
            log.push(log_val("utf8", &format!("{}", utf8_byte)));
            // have to do a stupid conversion here because of stupidity in the
            // original turtl code. luckily there will be a v2 gen_auth...
            let utf8_random: u8 = (((utf8_byte as f64) / 256.0) * 128.0).floor() as u8;
            log.push(log_val("utf8-2", &format!("{}", utf8_random)));
            let op = crypto::CryptoOp::new_with_iv_utf8("aes", "gcm", iv, utf8_random)?;
            let auth_bin = crypto::encrypt(&key, Vec::from(user_record.as_bytes()), op)?;
            let auth = crypto::to_base64(&auth_bin)?;
            log.push(log_secret("auth", &auth));
            (key, auth)
        },
        _ => return Err(MError::NotImplemented),
    };
    Ok((key_auth, log))
}
#[cfg(test)]
mod tests {
    use super::*;
    use ::crypto;

    macro_rules! auth_test {
        ($v:expr, $username:expr, $password:expr, $key:expr, $auth:expr) => {
            let username = String::from($username);
            let password = String::from($password);
            let ((key, auth), _log) = generate_auth_debug(&username, &password, $v).unwrap();
            assert_eq!(crypto::to_base64(&key.into_data()).unwrap(), $key);
            assert_eq!(auth, $auth);
        }
    }

    #[test]
    fn auth_v0() {
        auth_test!(0, r#"anonymous"#, r#"/;3mP%6O;IJoqn<I[b9r{KCNM&s(Ha1Qc%WNy..TQmOzb]UU>h)LTuu)U1R/9F[h]G]dr"#, r#"Tdp1tsJ08Y3PBmqi/RwpKMHn+qCjjVy7ZgpIgcrF2fo="#, r#"RfJmlHFqUqsdfAldWem03GItx6Mj01OtUJd/KWLzSa62OtjrpCR7YEl5mCU+usTQ9Fu8kLqFUsar5RmP8PUnwjJ+rJSZbDD05yk0s7OOGa4=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"$sV^HCk4{'4aEBU,n^:%<ur2.S :fyBzZK3PUU6yjMq*a.Y*;}.XwOU<gi*Wkt)VzI3"#, r#"rgfv8bHLXpUeqOdWT0zJV2JOGbYJV6HgMsGWtBaTq5s="#, r#"MwD+OCzpl3pI9Xg+K4Hhvm/iJQR1zpMGA0f8ZNt+ExPicPtfXsEdqnPo0IRfbEZjM+UQboQxG51Gfd3OKijO7K0IjyaRJ1jHFIWPGoFMR8c=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"8u%s6(@Bm$BYH'N?NgLc6SLF.?33e(MV>gJ$P!jtSXv%U 0Q01"m4Rl7{sClhZBNlxHFYbk7"#, r#"BW6h+tc9Q+FAiFtdXA7X/kEyNtCNTnxTJHTKocTDC9c="#, r#"yai+QcKtXHdTLErPZRlEUPDRfCeA4hZVCGwdHRb+4c3rSZxvhqlpTicuLduFsWaV8oC0dpguHvCL/SLQOZueAT6DhQ5LNn16gEb3BhfAnvo=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"c64YUE*9pvMLXZE2X?sr>.G&!WVs?y!y) c3(w4w4&<Gn/517tAy2^(QL?<Y'wmr3jhu"#, r#"VvFLCH/qtgunIgTYakOKfdsoC4/2s6qVoowo3ige9n4="#, r#"WzK9IewM3HpLJOoKdw6ersu4IFsMBobMUSLmympf/tuW4gH5c8PA9shruZ50vv8ovddTlDmcTakU10UV6OEJkg7JUFVXR0ByYuMofkwHYiA=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"Ot([/Kjm'1>%7@]n!W(M8bzg7O}q,L:xsH{$Dm3Ojm}K5trFU9DPmWcb.w}%HTyPXP2{]<SJs2"#, r#"5pVTEW5rZ6h5Og4zqZICn46PvbYqK5eO5TO88AXbwZM="#, r#"mZOYp5MddAIPMQdD9iws0d8vU/DntlMdMwRzYbJKSqrQfGK9rvMk/OTX3wwOspcTFiQs6FfqaBwPjnFbLKkS4sqEYeSTl22wmWCset94dMA=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"LyaA9{l{!c>T2;FV&InK&C}lI.@IG";c]lz6/rp%$ASf$q4]OKEKirD76F;j"}0Hvi"#, r#"Fr1nQFRhtts8nHVXs5n8T23EGHvf09s07olGoynBtTk="#, r#"IDlNAY5eCU4ez4FYWrjtXTDiZKpKPhHpb90NAUsVvcUs92RsM8wH6o3yMpsFBWFJ+yPmIA6k0NqjX3nrbXhjyi5cSfpcMILgCzT+B+boAcs=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"@&QEtl{vL'}cnUS0H(G<)H$Ar. c.c:Gxy0et>E0uKbzll/Qdt!qE](e#AVE/4!bO;5"#, r#"7V73uYwS1PBoTiKdXgRtgz26k2z+uVCIGVGyL57VkQU="#, r#"9LuxH4yaktoWKF0SU4D/GeQSvu57WwIoaFAtVzA/6Zv0NbCwyXEg9ee66OLhzzUaWDHOHOkvYMVxwOy8Fw6bJ0w+duNnsImAr1zdlbzXr7Y=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#",H)mEGR9mRbri}t{.0"V8XqcVdcK*'pq3"%>C.@p*maVe.SEMP}aTNy{PCGhfnjc{f16U&2xa^^"#, r#"yUXoOIUXtjCJkVNcJlG/ZlbnINc2NRl66lcNm/LOaoM="#, r#"OFYOV60E95gyOTjNIfuMvHRVDQuTvFYWj+Qc3w3QT8+4vJ1q9ulHI9bmrlSKv7jhHk67XQbx96XxSvn4gJJbHdWmBRMjONrieoMns9UeWUc=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"omfH'qKvP]P7"Yf;O'nF80d5D9d/M2EFEd:@cROkC35Czo/^(@0,*:I:H^SGSM5Nw"#, r#"DCFsH/4pyvdAE0S5hBPDzbSOlzLaTVydgdj1vFtB4IM="#, r#"DN1982pJJX672T04R4JZNGJ0V+/WVLAamYYVyck/EqnFXqSmtrH9kv3/ME5UckDPtcSc3YKmBheTDrbf6Sy84PbBXnNAAOuW+8ydFMdwn3o=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"MyXhoAQ0F,81IBD.<Vb#bQ&GV{SAqfl@.Z({D"Z!og(80{}MW"UZ!jTeFCJ)jhFC0Oa.]"#, r#"DKML/2f2iMVlTRo3bvwM3IHRs2EEJRKGUcT2DOsV+e4="#, r#"Av8Zoe0EfUEsbv4JH/LdXFvrrSLMTKEkYLIJ5dHVi+phYcu5VfiHIT1vzTvWD5rpYX+x1sGkms5agy7Z9vE1OizilJ2HL5Y7Ncp5Ln5Ub7M=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"RC [pN13j%387.6O*5KvZyo!BOC4VJz{G*4dwW48J#pM:(7]{VimVY"QZ(fE}hU}^UT{NDA$)"#, r#"9Z2v3DaipV9TwT9Dm2dT43XRVfL8J5ySrQ9g4kdmYpw="#, r#"kFvW4uVj7Jk6yxnElstnKWyz1kTxcS2mvZP/RX5Jt5rpG62017QwCqSrfYJIPbfOA0OLPDvyE2+pw8ueK9JnkBBFDqqDK2K7/7u818CPDn0=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"(9R:W.wMV?&K{9cFuLuA&:j$G,I%h(8(?"N[YMgse^H wI(7T"ses.G^/SoZ{r%!Qpn;g,a$cG"#, r#"4GCCMTfo0QyMnKI9No3P10N6hLThHvOUQiau2gsGYBQ="#, r#"HdPxeo2xOhJ9s5gM1EK8rq0cVQgF5i+F9a5xBytWYp1XgWxX+XCaKw3HdKnl314qiI4eeO1reEXegBoVvSMtF1X9B7X+Vi3KSvijnm0tSEI=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"O81*A#E$7oW!<Mxt7Y#%<)qGqViUf@tMf%4SQ9u r*l$5jmmb^gF{z:[8ysDai <Y@@W"#, r#"iPKlYIdIHwuPwz6TdhNiAaephJJs0jhSSUjqY2nv1Aw="#, r#"+YzQ2wQ8VDXCs0wTX9U6W2ojJb9f3P/R+ehXL3wmJZRmK9ED2mW6mdWY2Y41bu287juEpyELulNuLpQOtshW+TyHxzcDBRSKa99m0EqP1zE=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"R(*>N!{)S&DB[l4hiPaH(O8zcv<e7B> W<?1L]0EWSf];1Ajv34?lJSG1,No{4DleNRq("#, r#"05CjT5LMRyeBb8aLT3tFrL/gyrof78kIFSHUHg4pfwQ="#, r#"lBbnpInhIlyom5L/gTLQePGJo+f8C8xKLj5Ifrv7XtASyqWMC4/dKybI8Z8nDx4XD0QW+nERq8pcbxlOWQk4VAkjcTuLaIL5ZiCiowetHuA=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"l0@$.VV6Wn^{'8F3xh:(iEB(5rw>(z7N#%PcmR0@dBjJ$@S7}MfYu{Fkv3ZOd#?bz2qI%B^l?"#, r#"Zlvwr6gPdDrq3wJekED66MntBR1mvFcZ58y0FrTgdPs="#, r#"EmH597ZLUw9xAWfTZ8dL0IN/EJVpcPZagX8t08OQKLOHzGDopB0W0Qmot4nQmCWx0SlvYGTYO1J9Pv4SHAgGdB2ehwdVz0Fh98qQcU22bwc=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"g5vZ^xW7tH*2u*Ih#5)^D##$x8w8%S7cXM@T9.l*:aTZkUc6tR"zgbRHeTx;v7@fW.?]"#, r#"I0bXwPMosCRHXPtE7IQekLSIKzKnr19lQNYX4DRgA8c="#, r#"FNtB/6JWYUYNyJPxmD8r7shJFBk0COIDDLn4Qdf8MoaA6ItLrPQczAoCSGGtkK0Of9gBhqcai/Yijt1FEkUK1OAbc9NiPRAmd+4FKppHnyI=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"TITU8gKG6rt,2<!dzSV[5qpzRhtx5AZ*;^uaq^myY2$<6.wRam%.YK"?NDP6:D:U?#.sq'$"#, r#"eFwns7TGqKg6QwKjaMBdVDa4XvYUQxmN16h3WsXrjeo="#, r#"Y5aIKm3G8gHT5S9PzfVil7WHIuRCgomY/yjVoBYN/creRS9GrjH3uOiOyGuRog0t598CBanv9wI5Re/upfjfHkihITJk4BmAO2n7z/Hx1iw=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"4NdHCT9Yu<)6g488W;LA^F76Gpx[L'$UQ(Bdw2L/uWv}.X}2Z)(]hd6g0OzOS[{Ihff:%n$oa*"#, r#"O3qNTtxGU9qAT7wj0BJOI9PE+UnGMrUBQBG1281xH2s="#, r#"tX+cdQFMtH7NMpmqJ/1ihX5w4x0ZjgvZ9P20GIB3v132hpYhyLfFA7z/g7fGUmBaKe3Pj0qazyG28z/65CQWeJRAP2jSh7SsYHLZ4CPfZJ4=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"yYac>L9M[CGp/o%fe/D4cQOC4,p''@tGMb(A?ae1xOwFGc<Tx<(]FOc*@qwu$vrE$n[U,"#, r#"zDRqurTxaCPta82S37yh5m2FkQpTDw8WZ77DbYuPvww="#, r#"6TMS7uFCiZnz8JCsKhmCTP4CbWUY/6UdpXHGOkYuzbU/uaSK17BDbTaRz/v+ggE7cOuTom8LbE00Ld7s3B0WoPgwji1rBuoC2WSMokUqRBc=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"?)&TJ/xt6yS#>3L&1@PB9AL'Y.:$'!X0n*[?W{[L9PRySz;jf8Kz:PQ"ysHV/p:2&R"#, r#"PmuoJaFSfC6icrItHVDFkrv1WiQJDrAo1aR9f25MqDQ="#, r#"LCBmYNs9DxRp16uDm/9m345rfcnzhRkS9YBqDZaqOJ/JI+Iz6UDeU/FkQ5/rDoTUhZ7c9bLdnDxg4E6oRsi6pH7sZ1pBr5cC4fFV+Ua0lLo=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"V@"R[UE8D*E!xE;9vFyauz Lzza04dLs*l)YBK96};<reCf;7"&yt7bnRRq3*?ABa"#, r#"H40drQpyxchu0edRZcpXyBjvK7IyzmacLAmpcX22kG0="#, r#"d3gR4erXoj/qxWFQZxf6mkAefKFUTRHzX9jxFay7SRDCLrkKASBOfuIDejrBMqbIwcTfGiCAq729itWh5+suA2ZEr/DXUq0D6R+omMyYLVs=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"q*6u[[]X!>0B%]HQE$3e>wk%.6j6'p9^Aa8'/)h >n%3Ilae4[VduDf#2kCgMuZ)keP''"#, r#"6CBW7GY9JB3g3zg9NWiiW6CtjO0xqbPFvyeiV+Grel0="#, r#"X+iBDPHiNAbyHKgly8zaVaqFf4Zrh/HpZxINCTT1iFo5hJrlRlvq0Wv87B/b9RCXbQg6N3x0qio4Ih1SRDbVi7k22Y/Bh7LdSNdtQyyqvX0=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"C%>9j2My(lzTKe}J1lTE>krmG%!VF{y1}to6bih<>K$W]MYli17d(l@:lo%;QDDEw^MNaGJ"#, r#"f1b5eibW8Th/vGOIqKX5nSgRCRop2MVT5cLpakCv2fM="#, r#"GrQFcvxvWwyf4frser6w47vjSpB3Q7/xiBA4Zs4/Y3BKWPf0QWqEwGZ3umjXUyMf1qN6vBgGy1EYObnLqUhhm3lg0MjJKctUk6qdYjzLi2c=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"Yxa4"N]Por4YRa,K"W"XiKHJaQLQG &: zhMH?6ZPa[v s*:MSOqgp;M)U7UiMnZb.NL>!k"#, r#"8sP1N+DoH4FfX6eePk0FpHCofmTvFGlGmMEPG7yVAV0="#, r#"+yChuBKMBCiajTm5fSLFNkFqqRaybQxG0Spf8J46nO+9YlfIDswUaqpsDJS3pnajWmkUTCGzYTdlLwxo5kjpY15Le6DvmeLCjiGXe/hxWXE=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"j"019)D7<!MCamU$$zH}DRRP8$du5u<c[?dPQ7S7O&!u5""j97O:}If<0,YrV6.#eq"#, r#"LeUSKX4K9uT3aa+GUq2DpQ6FXMGdeQCyK9fLdVq/X6g="#, r#"LlHdrdUuxgv9j8F8y0weK/Sy5WG8QYl+DjXzPpAijdMuxAzv1uxCeg98Qxmprg6Whz/yuaLnUH16tMtUHgeHH8zFHjvVOQWwvbK/IbEi5ms=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"MgRTL{3oedOa6ZYTEL]%ci j;&DI HZlDh4QM)R/PPbCS"'7EJ;d!HfWiI.683YvD*1"#, r#"RxvtEDIa9OQxYMkTXU4HtkJKqSydm/ZYOdjQ6S41ws8="#, r#"sxBTGy6NPqTED84FOT/EzKzpYNsi1DrL9l6U9pAqiS+TumIOwpxpjXkYFHCHl8EHspNAIIyRs/TIQCafBwXvtvl7skhiGjWXEbHQHBXz9hQ=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"]1v)'; %nMugQ,K"*Oo5PZ>j.QbN!?;{caZ5gnYv8Hg4W:b9{1f0M4nqQ,.MFM?f#T$%X"#, r#"kSDgjoqeBMuu5qvExGZnERL4iJTQfDw7zj5DS+SN8Z8="#, r#"lAycsc89oOUl3I7FzSgtXD7Bzc9Jc2bYkoxV0AhkmX0A2O2y/JXru/fc+3tbd4qph3JCZuISjzxFOhq07lSnaBfSRFITsR1/ySVCZp7QRms=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"%S,TEk"(nJZN^7d@NyOq$Rzx8E5u?iox$Iij;^41PmKvZz$MgtU chzN,!r,,/FFST<"#, r#"2CK8ilXOlZeLbbIC5onKclgv1w3b4sx7IZPTl4s1XCk="#, r#"drrzbiHKk5N8g8Ee2sikd0Ff+JKS9pAh366nO8WBxfuUEcvVuNkg5IZVjSaEPUzDOmiLXL6E+EI/CGd6zdxYQ75RrGjzLZgiVnrS1My27b8=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"r8byw7AM^(PtI2u.RUynM6G#;qd!M(gKF{)mh.&TAY{qruqye5NbTwiQfGlf%qQxUR"#, r#"KWKSWYri2OszlOTmb5vgUZfM1WQeTih7nGEVPW9kLT8="#, r#"CUh4F7AsWs8MEEWDfUFQ3dQssV0rjgM0u/8KQ4seDgQ85nZP+vRnhQKbDilYUERe9XMeqBAdBkCup7aT6ZF6FTTFVe9gdmPgnfNTq0OUjcM=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"<{:lW<2ikNL2mC5$ c$oFN{C*z>'Aks,F9p}uLSJ$wq2'a:R544,9:GXjj8R&#b8JA"#, r#"I5/XbVcBr1hVa8ow6+Ke7lBCcbFIaI5XLI5C9x1R698="#, r#"kX0StckaGdg5cM0nJn72eW121Ut0WPGpclZMqRTaydFGJT1+uPzEzJFhHKFTOcjuDgU8FcT0PmnSOkaTkUbt7Vs2kA1NLG/3Jh/e1WG8wb8=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"3k(O{m%6KhVf&hr"!uLI>&[aqC]H2t)$2Hl2}}")!IIa&GvY(>^VLf/}qR/JafGa,>"#, r#"HPlPAs9lfRhu2fpq53u+UgRFPtNtMR65mAtlxJdvk0o="#, r#"Hp1MV1bXUmXW2Vdx9L8EUzXxb0vFUj4+Kivv1yhBBvQgMIp8GOZtJ+MpPiKRjUemhCsNINTJNWReIVIruMxn2iKVm7L7AesXy7qOovWZOiQ=:i616e6f6e796d6f757334633238313938"#);
        auth_test!(0, r#"anonymous"#, r#"p/C*F:[o4QYXHuv:oBwOXpN8M3fe;oWcMu0ysHy&Qa[a}5e#0MP0ghoHe/4Fm0gIzaJ]"#, r#"DYLfbngGGpfW0dxhZRBGKWNdv5XDs/jOZulWeQ4RHMc="#, r#"I+vRxNtlozW99B2lsVGWnWJdNojT7hrec0byZTPxUSRFLV9MscgOm0ztfszEAduBOuJ2GyleCb57vAD0BXC1uwQMPgLbgCSh5QHz7uZAmiM=:i616e6f6e796d6f757334633238313938"#);
    }

    #[test]
    fn auth_v1() {
        auth_test!(1, r#"anonymous"#, r#"VpEF S}nFK{K1TbfWlFf8";,ZmP@c1sm*v)8yWr9lJem"0gC#k'dBjVjf"G&p5Y%Bn, "#, r#"OP1myyAQrmz41JbsaLcKZjHpv70VRjE3fE2YVn4dQUg="#, r#"AAUCAAEzY2RlNzc4OGIxNDYyMzkz+vuFTwhMhgpAqWCclH0zPwSMPombUk2EwbcnLe6kSYwhL2SicOr7Faoi3tH+KD+95nUMLtg0r2vNddQGEiGbJ/J5lZzuF7+EeNv0l67FLWQEjirb9Zx5EbbzkrkNnz+bBiRnO6kmlGpIWdAfZ3bPozox/QGICl7PZMSE73FdO2fvgoO0+hvoa94Q7crnVMFamvQ="#);
        auth_test!(1, r#"anonymous"#, r#"zT[I{&UEQ3OCb*T>>MlQBsW}{tX,2(rojZ)Av<FB,P?}Xg@;N$U#Y/@jRmu!{Fm>''?$"#, r#"ZIAb2sSRZT8tFS/RehNt7ZqBoMqPjN+NIqBEQhWgQZA="#, r#"AAUCAAEyZjBhYTYxZGQ0MDg5ZWRkKfb8dZu0LO2p/VcKe9yOXoQGYqdVMuqMCtzOOyaxm779wpAcC5v0MEthua9bfa3e1DiWCvEPoDGNShAydE3N78QCtLdmvp1tRC6LgZqG6/QkGcG7QZNlTcDzjnW594LY/dMr3jIDOrhMe3RDWhXgHioUE/mH4a9CtscFybLnPKB3zPPxxrNej7OZdEqdUC+hjvQ="#);
        auth_test!(1, r#"anonymous"#, r#"vsUF1x}'P*n!%H&FsN@b*^a8%aFl8qrc;4tTr)]Q%,RwJH#Sm'X9'd24zw5)Wl(fyqS/g;G"c"#, r#"7Mm9F6MmBJDuLrEY0cr26ahFX57xGiOtjSzseW8dO9Q="#, r#"AAUCAAE5MmU0ZjQxYTg1MTViZDgzMCAlHUWgF+CwOkGK4g3BLINj9zdh08uBW7nEjeFbuLPjHrT9lhcmIZjM4M0NVlyRmNzf/pRW/lEaycWhF8bSkTNtshogu01OOn5shcdHF4xrhP55Uc/CQoQj2n2+sEUBxORTIaZVHqTSCOS1qhqhrHcn085lmtKxYSdLWvB36HSv3rYcRJa3NyKPQXhlxtZaaPM="#);
        auth_test!(1, r#"anonymous"#, r#"82S>nxm,V!/C[>@l]/NviPPfUpv5 /j T^2R{P3LDf)'3q]*A2T>iQ]dd;([f1U'2r""2F"1BB"#, r#"Cg2niL3IG3dwiAklMst27DBsj2rWSRIz3cJAhPszE9s="#, r#"AAUCAAFmOGFlZWQxYTQ1ODI3OGM0JYM2JpKEZDAeNq4sYoz8GyljrJbi994KJyPgsweEdGu+kH2hmWjio9KMdaQDqA8HhjfoNHiA26MDteBx6/gazzwx/WWtPb/VXR2iqZsu6SikBNkc/S75INQNbbD1sN1PqRgy+IRn3YbVyuepjf5RpgXUTDBvjSX1sG2SQwmwguCrzL9YG8BzbiH8ynUJfL47738="#);
        auth_test!(1, r#"anonymous"#, r#"!GSg*y{g&0>C.bBqQ$]!mDnO"5&fuNZZdaZg?[wmqEd;p.;P3T1*jNa^nKTUSsqfjPeS)2<s"#, r#"nAkPYUXAK+p9O2Rw7rK2xs3R/aX2S2+wkvZu8G7ubj0="#, r#"AAUCAAEyZmI5ZDljMWM3YTdkODA1HDkq0qPDJx0Gdr1fRwq5UpoNugqFyveEkHwfNjf3eUAm9i+BJjhuSISkIySVzGtOkiFIiyJ3wantvKB2ohDrsAu3VkEYeIzcqBwGXXjNkrpwdSr9srttsSkeJo7nzAhTAsoMD3K/kEIEs9GU0uAutgWWeA1kKzCuugrUcyCt6x7GmrxyZzahvMokm4TrEEs9B4g="#);
        auth_test!(1, r#"anonymous"#, r#"9@%Oh<4oE)q{n]M#;E*Zr'3CJ'RyB9T{CnQyzockkx)x%HMGwpbS$""<a$obiaGfe"#, r#"AMyHCbKrNp4iS/BE8f+J9Kb52UGlF2Q6pNru7oaUu/k="#, r#"AAUCAAFlZmZmYTFmYmY5ZjVhZWQ1zVjaR7VbgyK1kkFIh3yCZIvNuSIBHnV4FN/v/TT/1g1bMF7QEK/3480sKAv0NBO41tz2UgUXSkSCodwp49mfZllHZXnasbzB9FblwJzK6gaTsX8D8QCj3+7qdVf9hd6i7L+kBFv1DJZI3pADaEdvnqU8TSlWnuV6X73oDozd1F2Z19OlYJiR680Ng0Idd/LuGfI="#);
        auth_test!(1, r#"anonymous"#, r#"4*Bo0f:@w4E2,7kXGNHhY;xW8%Q8qx'92.k>6PH(F2ZJfbQn2U#{4!3HncWZF{VB8Xe"#, r#"bTBLcUI5RiTe/HWv/7Wf6/q9uVqXF3sNFZ+HOmiZZXE="#, r#"AAUCAAFkOTZmZGRmYmY5MDBlZmE58EtH0zpl53RVnGfZWxY4D6JfY0aGnPCGctnoASlwLowrgROionTqHQjBhrWrqFZcp/9+V/A+OFP3dFEcGFqJ9Ja0iKNKBs9Mt+aEzwsj20DWKl1mN7UO51Gn7ex+8qVnJlEG9klQ/DRicCmVKYfrLvmc0gD+2lz++7aJT4Vr0empURCT3m80NUoL0adI5jgc/y8="#);
        auth_test!(1, r#"anonymous"#, r#"2J6GoqwD:4enguR7:D163h5HHi8sN R![6g:'Oeq@KDlh7%!Iyt/NM8lH/CX@g:vBY."#, r#"fuxDAhTULm/ytpyxdrOJMI0sltGZHY/uEzxeRdE5K3w="#, r#"AAUCAAE4YWQzZmJiMTQyNjg1NWIzWnzSY5XhuGpzi00F/pzMlK/00iQLQJcLQqQ4FLNqzXyGxFERrBLmorPkpe4gslWbk/VwYE6hw6XmzeZE3/j7JV4vhqHEYoGmjAR8yZYzlrHYcnQmdJcZt45O1dG4eJnVGdCVfUWFvHU5ruEr34v0TJvGaYxDr3NJ4IBA0BvH15QTFEVh+YGsgHdznYprJJ1wW+s="#);
        auth_test!(1, r#"anonymous"#, r#"GPY.si<6ZSdxSmOpx!:NP&s<]t}>7/Naej:O"82hg%QpV>TEs7,1cR(B"@pNp"%.3XgKS"#, r#"ya9L32UYZtC8JRVmJ2qMdRVC26D9YxzDVs0z760s6dg="#, r#"AAUCAAEwYzBhYzUzNTdlZDQ0M2Y067dm7mB9aIGmdg7dTihY0bM7/Eezilu3T3pV318VaSGMnwsdLsLpJn0mWcHQ5sSt/w9cGC6wD8eKutypaWacyrB6lqzAJcxvnGHIutbV44RrvS3krIocPh4gckKoJldTZhyd8nXpRHxPRjggp916zXPvKv4z4w8owNciqTPmUB+DR5WIxT3vwvPj9UmGa4mHrSc="#);
        auth_test!(1, r#"anonymous"#, r#"SFj]hLxr:/oA^z6j])%7uMh>Q!!s]w>Ry#E#rXb}w4p: DaPc9.!ejpi>6{RL[cDjJ.[#]&i#y4"#, r#"holc9N6Si4Ako2+LEX8rHV4G9mHTRov2umMH0DLgf38="#, r#"AAUCAAE1NTZkMDM1ZmEwYzIwNGFl6Mb0l6l5DQAL9zGfCeJ+uDcbnvgYniotA2oQWokGFHCVarY2X9Y2+JAVOZUaBTCLdpA4jTCAxDVcJsC55jNscFzhO7Ey42dxWIHJVbC5fCcR52uNCD10JgPXJFmFZdKqyaoeoSjKi9qa0gHBU0WQOqh3BsD1skyFlkjGMkc8HmZUpUxqrq2jY+lmVC3axRZ9fWc="#);
        auth_test!(1, r#"anonymous"#, r#"rUCe0?'L'{ctI/e0Ye&;78m3<p7XQcyM:Llo{XjVmRdc%T%WK]!8,$c.)[A)<Od1uAbV}i#a"#, r#"ZYdnBd8ep6J8/UvX4ZcgboyqLrrHs7PUS81yUiULTf4="#, r#"AAUCAAE5NjBlMWJkMzM3Y2FmYzczGeii1mx/qtAk2GpXEdytqjNmu4R2mU0F77dPuIXP+sDvMooXlWqj8Cxh4kWXyecjO1yQN+jV9EhrBeGzAE9TmqN9eeQyBbWHQk9EtxnhLOYn5pbnUEUG6osfthErSIU6mJwYjb7UNpPErwSZifoZfpht/PAohmxnFp7mBKOtytLLOmSnGro2Ljyi9AJVJGpggnM="#);
        auth_test!(1, r#"anonymous"#, r#"!RP%ns)n{;X&lC6K[l$/u{w]O8).g<R$#]3u1usrLi7/%^6cfmzpB^0?!kM/fDh>THvEF;x6*"#, r#"/IjfcEP4++dkGl5GcWaIhdLROthvYOvu5Y3y1b3LmXA="#, r#"AAUCAAE3ODhiMjBmNmQxMTUzYjFiOPhHtUsfaS7n1xGEYygbAXYaPVsIhE5r0q2OpqmJDCohgY1uhLAfjAxVGS36WlPAZXSHpRBJG7vYLTkM/i+5xapzo+pYWhoTjlImE2aOtZ81MhNneFTuiBWvaxV41WRUoiuQXoHwyGfjtWDs6AI3MaZ62kA1sRpwWFHE6qnmFx/9/W67rJspBHONaaGabUFbYis="#);
        auth_test!(1, r#"anonymous"#, r#"P6qq'H*zz6^wq43L>N}NU5{SRa{@zLHsczS(J^#DItfE,G>tVo9iyM5v&WBf1k# b/3lJK"#, r#"cRVDo6MUajG/njkLdUnQeKpK0fqZahobx+y1o3+yzZs="#, r#"AAUCAAFlZWZjOWQwN2JmYjFiM2UxzMnUZUmM5OjH0pNtAxiQW8ZWLLYFAUMvuV34Il8MudDgq2vTfg+pditdDtyk3xrEwrZq0sCh96c4takwwaPvsNKft6motzK/qne+AeqB0wyNOYvwys2kFOdte7GBss4nRwHs3qRrjXuMZC+MInzwiYytdmZQPDsatCNcMxGhy9igALXDmwb9BZS1POuCVsh9yGs="#);
        auth_test!(1, r#"anonymous"#, r#"CEr@{L]Pv<cuP'^Y6Gd1}v2T8#s^*9"?UnR/5A%#TAk N46%vhZ>BC&<xfB*y>hGm!f"#, r#"melVbvc4xuf2Z7QoL84rmVn6LgbGsZIfjf6tHsed7nw="#, r#"AAUCAAFlN2E5YzI2ZjRhODhiMDgy+bl7zEzz7mbzz0iGGZZVmi9DyuZHOJ3VgJ/WNKNsRoTXSac2rR+tbhd0olUnM4rjf9TIbDMtWwmcbuM2t0Ip0f5wPAsamiOKsKVHPgeTZeGeUTcmPHLyojs9ltHYDET2OPa0zhZuRtqh2LKl8ARgV3IOXpJzHsLWegVMLFGdolPmtw6PwgBlsGmX9KTpxx7IFlQ="#);
        auth_test!(1, r#"anonymous"#, r#"n.En^9<{ndGcIMg^)Jdu,VjreGYUSVA*zV}G1u4 NixTxKIK55 [wm!gHnvia3&cOaB^gW(]W4"#, r#"fDvqkwYIAI7jAkwAhVPn+e6Ry3v6jx/8LcCaIToqkG0="#, r#"AAUCAAE0MGUwODUwMDIwMDVkNTVimGoCL3dEtrqSL8ZkcDF7i52fsRB8pSPcLa+ARcyaxnvFB8R+bCS+7G1iHtZ7WS43dPFtAuAjdvyUOylaAmGI7E1LmkIPZcMZ+3tyvMNqlBl3/6jBKEBjlf1A2dJ6iJt7Ki91n+NEzpK8+Dq+BtRIpLnLV39IUsvMxgivNLE7khTbBSh8APYhf2gpw+/DwfFm+YA="#);
        auth_test!(1, r#"anonymous"#, r#"P7U]T?.AfFIr4p2w?:'pnl<P5kcJqf%]>gT@OjS?t9XCOpv0Q1ZoSLVmNzSJw(o}Zl"#, r#"+p8/ESZxmpnYtE0nMe4R2JNpqBPqyyhubRxVlyIDXsU="#, r#"AAUCAAEyNDEwMGRlNDUzODIyOTg3+C0FWhGm76ja6pQKjsWb9OTTcJcyhjpdz10WuKsfuLGMn4O+TjHBM39ICiDCzbI4MhsNTOgySUy0haZiRC5IRdL6BW+2DdBhTUU7BoasoOCGK9Aj09+jJyYHR8UHwuXHTM1IEEjkteYh3l11ddpcuEW7cNuZos6T76t74BbaCwOZTn20Yq+O0gbFROKpsLhSbO0="#);
        auth_test!(1, r#"anonymous"#, r#"P)9MyrzUSuDk'.zbQT</YMY>nZtwt(ZP1W7K.Bf%1UW]Yy:tQvMDuZPW'NaE&l<XA/!$SXNc"#, r#"cq21Y98fLkd0OkTJ9dCwEhcnj1me1VRR/vOp9qqEkaM="#, r#"AAUCAAFkZGNjMWMxMGU5YzNjMWE2PFbdYOE/IRNfgnKUW7LtWiMY8cfuh7UbYbgVf1Ly7VgkrNgm+B9AgB4Be6rUsO/Nw7AWu34PFXAlPaEBjjQ85upApu1u10+19eNKFFDGrudX8u2X2KPCSz3yWM0JGNJXJkl+ENT7JVpomFd0dR5FepqkXLCTkbZecEL7hjnt3QULAOj8DvqRfwZFsMDFxpP/bCY="#);
        auth_test!(1, r#"anonymous"#, r#"!,4McFIRyjVo?1it.0pzMTkpzG}!d}Rnkw][ tc"7<l0*?:5P"!fto'L;*f GG8lr)eo!G,"#, r#"gMKqHO70DeIx3bn6Hasq96aYmL37lINHQnFQyUdhQj8="#, r#"AAUCAAExOWIwNWRjNjQ5YjU5Mjhkam9on4HvZSurIiXyXwtguZePYVVu8CPpOSWIV3X95usws+PklRrCqAvAGtKUDmplPh7jhxAG0NNQ6rU6B8b/78B2NHPqNl4wM6TuTe7VnUiU8lwYT+kB06M7raX+WEg+JVkjAskYpXdK2ZePehoRL/khISziVO9AabZ8ovw92ARrZwCFsrzGiqH5ArdU97Jk8fk="#);
        auth_test!(1, r#"anonymous"#, r#"7v?j*Xi8eQ,P];D&>Ccqa(GRH'Q^Id8b{dPfTb kh;I^tkMVS5VMEV4Gox^'M."7fE(?f1cHy"#, r#"h8l77A8tLHpWWmQaYQm+kHryHV+b8l5r2WVw9W3Mi+E="#, r#"AAUCAAFkZjc0MDk3YWIzMjIyMTMz2unSMz37tHuGslEHBPIH7VJeZQbms5OnaEhHSQW/l3AkoETiaFTdNQeLdHxHnkDx9JHvz35f2KuSzbg2hxRNDQSdJ9pHEw9fqdRFbSwIYq+cPEzE/Sc38ms5jn34fpcPVO/a9P1y5LSrJfl8doHGFXu9jPzmcPshbIgTIMdHPMPHIs6YhUFim92zw4hJ4na3u0U="#);
        auth_test!(1, r#"anonymous"#, r#"W}O4$8xuH?9rh%{;'mnF7<d'w(&^9'UK"TS0WXtDyr9C^"%j}Q<nV 2UM :.U%}{j)[&@uW"#, r#"ROrJTZwtqpZIm6Z151JCLyQ4YEiJ0yokK6mTMLaZ1Ww="#, r#"AAUCAAFkMmY1ZDFjZGU1OTg5NjY5giRv01n2y5TF6EI+bb/d3bKBuZbbSF9eHXxfAiLTda/qtwC3xcJaDkPZnqxOQ+Lcbp8aLUzZdoThlgiwJsja6fA/C7gc/X3tpeuSKKrkARFHLubB9lxgkLcwcs9t+d/9EMK7gS1ZZVsuVlFHag8z9GOUYZB3L7F5Lp/YuvVDGhl4NBC43Hj8MQqcIy5y5iLku4s="#);
        auth_test!(1, r#"anonymous"#, r#"!]H {WWQiW%60JEgn}KL7)u3*U1CajhM]:"Z5cK))a{P5#yV/2([k"AoF&L<G)LHnwT6 "#, r#"Ta464zaPzeT2y2afpCv3II3Wn1qBJs1RuNJ/AQMbwL0="#, r#"AAUCAAFkNTRiMWRhMzcyMDNjOWMz6M3oYgodHIJw6ccFFgmaglWDOso499p3JQC05+qr995K8kxyIn535SCzewMz11MkAQ6ccCsbtqa0hOQmVmX4BRVmaiKngmYvIWqH76oAJ9k9MOvsPbB89nxi2lL3E1E2j2+vo6kswzHhAWhdLHt9mEddmYiXbq8O5xzmXut6X7gEqiWOXlPpzB5+bMTaIgz2FgM="#);
        auth_test!(1, r#"anonymous"#, r#"3rXXIwsVfT.KDgG{W:LxD.onG!L5q?io<xG8S!b{c!^Vi/?$T0]^yfM*j&6f%B'##SS/ce"#, r#"gpQe0BnFp/vTDHooUBw0BuhaXpuCI5Oe2+mm4hLjSPg="#, r#"AAUCAAFjNzI3YzczY2Q2ZWE2ZDU3k+g2OH0LBoIM2pzzCGpwbZp6iLxdOD5emX2/7RCzNobVdgOxIEK29GD2yOvgUW+o1Suxz6DDLTcmKfiqHR0bX7UoXCHe5ZLb7JGuzGB6XsjQutBhW2mk/TFEwV15UaiSKcKWwmdWzTnDGQ9r/Knqla+ZqMx7+O8VFMt77pvW6TM5M0WswloiNQm00PUSiATHM9o="#);
        auth_test!(1, r#"anonymous"#, r#"HPw]S&T8v<V<lpHFZ8[VIBAeqHc$6?%dqaN5]z[B],^#guLx"1gLQR&iKKy6Rk5';@P sKt,"{8"#, r#"Xub21OdTyjtcHMPHyYpP1sblnHVpBHzuJR2LZ7H7/QU="#, r#"AAUCAAFmMzE4NWQ1YTk4NmQ0NWIzWUzMFiuSNvZ4YZ+YyM78pgt2BvtFoAWaY8eojMLNHmesQfoThDYywnTVxq/4msOLjCHnFoLIopGQriM/lWaf4ks3RTtJ6Ix1AXjUE8XS5ndH8vJiNxjLr39lJeehi3rKEsgvXHhSqdNF8w3rsaqyomVWqev0yuiYnHeCFOr7aT+OPNS7bLddaaoVp3JH2A+3DJY="#);
        auth_test!(1, r#"anonymous"#, r#"oeLG2sxH%ACE2$chWr]1@WEu7^QpWFzJfOD8"<qYh<cKj@C]JdBNVR{xLu1JB0#apoGGs'w6"#, r#"5gnua49elZZss2IwOskx+kXRqyBumRIM0El7bJRyzJI="#, r#"AAUCAAFjZGYyNzgxNjE5YmZmOTBhWbT0+FOZuU1eFJxBO4pnPyE3dsR4xsOuAjJlwIlXHMhkYSz/u/boG1Hz2ci2RJG8yrGmOS716+Le/WlApObFUoqA4MsKQaKVWt/ji2XBTW9duFSkQ+TiD8vuHDwGgw5X3znNMculcyS8C52vnX8Rp2PqazAekNRGAWvyzz4fAXkiQkHQJ66aODGBbFd5bWd8HXo="#);
        auth_test!(1, r#"anonymous"#, r#"cT2Np*io4bBcMcnUbItI&)e(H"GIE5:n8fphH]5sk,%{39S;;!j}XS$kQR*0llG@b5/%mrZR"#, r#"uT9PvWvqmbJ1j2szNSwBhnOTq8/iD9lwIXDa53ynv0E="#, r#"AAUCAAFlZWZjYjJlYThjM2I3MjQx30igRVuZz7DDbBGRJxACEVeuM0ySejKe6W2Fz4sMgHAjacFO0amcyY7BKTVQQ9Ye77xT/akSUfbIWabg4LRFBhdVvpf08N8HMAPFtUQbndfkdfr0+6OAztDuZFYFpHdlymk4ZG5aRdM+xPdLW/te1ogqCduasFPBGfxWBr8dFHIJqSRGXExvNNcgTGp8664I0VI="#);
        auth_test!(1, r#"anonymous"#, r#"4*4J1k2H[?LS$M6.o9MZScPKr.tkHFOaej>7/XNF?]XjBAn"x.y0.k7^qa>rm^XhY17m0tv"#, r#"sCTILTHGbXNmXL6EOVB1iyUOZniKcpIMOsf1PeOmua0="#, r#"AAUCAAEzOTZkNjRmN2IzMjZiNzViTZMdQ/fxFIjR7GYsgNra4CVMgl/LSeEtLbagIK2WgE8tqyFhxyU51kWH0yuXLx7JJEHESC3c7DJYxJJ8d1RDHbKepDtY0jZOaZ5Ms1Sf81t25mnP1LmU3LszY3zCvFuNu5i91CSE5ZYMKn9bVw7k9IQ4/TG4SvlwChZlhVzunWfkLCVUwKmYuaBlTv7tMKVAMPA="#);
        auth_test!(1, r#"anonymous"#, r#"C*Nz4myhQ:;L86}Az?[Zm,5InpJO$7#muhV$>$h]c<u SI@k0p9nO53w2fk#w](JQP"#, r#"SpmlX7WZxe7uVaNBgn4gW6pjQtYEzVyyJ1NbqTQSiyw="#, r#"AAUCAAE5OTI0NWZkY2I5ZTBiYjZhXym3eTMzULp07pK/wAu4oBgiUolDZNwOE4WJqMlV83VnyUoVGTyfrr6F/IK7/GzqEcdZeD9cVU5o4vrDwl1xvAWJnMongOwjw+qTX6WCQzNNXHdUjmJM2DkBoq422/YWG8U3mSxekZ5RDstYinaEM5tGAax7QNkR5+6iOIDg+khguZ6rcBfxinHaGCoMoT9circ="#);
        auth_test!(1, r#"anonymous"#, r#"Ad),j9m#qd{xf:(I?B<GDtSFBI:m> ,(S]AF0#sLn&A*gVA:L.UZ"tw$.5 x:L;4xXWWL"#, r#"+4LgPWZFRPLFHP9RMzFoX8Ni7YN6k/sv2pShWp+SqE0="#, r#"AAUCAAEyYTg1YjdiMDVlNWM5NDdliVE6V43xoU56cj+6k3PvY3SdET71Y7e69nMj0oimDGxzqhVbtps/PGE5gmT+fa+T/de4U3yP6qicQ+4+j+qqui5Z8kfHhiDriltk7FxqxVB4jDoGk9tfTbVry/mjlL+YWxfOtigB2lEd00geYC2/8j/Sy1/ZrfkEKiMIVd1xbZHpV5AEmLmLiExXUbZskpb1tDE="#);
        auth_test!(1, r#"anonymous"#, r#"PBr!gUY j"cVU!m0L4y:&&Mg<nOy$EOU<{SM{sV3O:5D7tqOpGYBXMFJzF<c53gc#ZeRKUa"#, r#"HTnTaN9I/Ra4dQ1sW4rbY3QZ0pbd/HfZUP7dIXsKKfk="#, r#"AAUCAAE2YTVjOTA0MGM0ZjhjNGU4NRu19j2aiAeB9bRTQO4eT+S4iisCFbCrwygw4xB3XyApnhzGDqrMl10/Q4SyS+dNWEJJWfF8956VcBpvIjZmMGoTAkGQTLUiqa0ToXIeOcxZWblOS1id/QUYHnTfWfxyWqFnXbgrM5fn3qoir1UARvD/B0Iup+bNIRAUn1d53KZSDc3B/rA2QEA4fWO5bjTC1dw="#);
        auth_test!(1, r#"anonymous"#, r#"{vTDHlMrlx6$NfGpepmy%s?f6f[^P@*psj!'pw: .tki0VNWB<?.,uu0jhzI4bKX3CcIw,SYs:}"#, r#"iWUcf9z44UtWXtRW8xl1LCOifNlKJXfrDz+ZkGZx9kI="#, r#"AAUCAAExNTgyMDA3MGYyNTg4N2Q3pIQkwgJJxZLf4MLrHNDPCGDMzhoV8epB9wWS3VkvahcNJHshptuoQ8Ij8w8Lnc8spCDVbM6Tw+10SB+3opP4Txg/FPq7I12LLm+mbWiqOxp5jFkhoLiEie5Gj5uDWg+7RsrwNKeI/uRtnuttfKUsogPAIKTgVR6ji1YR3WY2ZZM3nQO3KHV5RsTnPB7Qc+2filA="#);
        auth_test!(1, r#"anonymous"#, r#";F[x2"A5W^Fe$aVgd,.E25jme2W["m6RixVakQe}?uVHMgz2LU'Ks1"aWibCnW%UjGiAe"#, r#"2oADSvS5Xq8f2cbF4WB0OqQL64WLMBCAlxPf4aOhFeQ="#, r#"AAUCAAFjNTEyM2UxMDdiMWFjYjk1Fp2vEbmQ5rMW4cH0OfnQ7SN0QbfX1HZkPiEnR3SLRpeWVL/s4vWww65m+g0FoZ1kzDmWjRNdTBBRNPfmqp+Po1+/gTu+yV2Ts7mnH3X6UvP0PGXZqEOIRI3exF40KILgemrIrHL/mqznHnAy22iK8jGbVmCL41M5hkane8nbVLRgJCxQEQMII8/vWcntaRtfwlY="#);
        auth_test!(1, r#"anonymous"#, r#"H<sZhQ[1U8xs7#s2An67B93 5#[o/zdCv5V%,3)t;71sfc?Ra5@xqk:btb&r[YRYY>FxXD,2tv"#, r#"xph6Q3dcKMus3x2jx8Wprrk/G1cSVScQvQxKO6Ow2Ak="#, r#"AAUCAAEwZDBiNWU3NmUxYWIxM2Q0QEIN6v0kMEfJYUQ49JMItxXunG133M8wh14RRgDFePbNtVilxav5bGmVc486YJLeM4+HmNMMNGlLHv2WQ/3OTxk2qL/WIk47L5jA7oCfhKGcKDLSS38wl2pSmmWz0/S5obn07E5fdNtf3HckA2lKI9758f/J5mQyEV2dNs2aY9JUoPq/DnhC09Mc3eqKnllgBvs="#);
    }
}

