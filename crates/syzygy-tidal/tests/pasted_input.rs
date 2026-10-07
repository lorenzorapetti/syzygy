use syzygy_tidal::auth::{PastedInputError, parse_pasted_input};

#[test]
fn full_redirect_url_yields_its_code() {
    let input = "https://tidal.com/android/login/auth?code=abc123&state=na";
    assert_eq!(parse_pasted_input(input), Ok("abc123".to_string()));
}

#[test]
fn code_is_percent_decoded() {
    let input = "https://tidal.com/android/login/auth?state=x&code=a%2Bb%3D";
    assert_eq!(parse_pasted_input(input), Ok("a+b=".to_string()));
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    let input = "  \nhttps://tidal.com/android/login/auth?code=abc123\t ";
    assert_eq!(parse_pasted_input(input), Ok("abc123".to_string()));
}

#[test]
fn bare_code_is_used_whole() {
    assert_eq!(
        parse_pasted_input("  eyJhbGciOi-xyz_123  "),
        Ok("eyJhbGciOi-xyz_123".to_string())
    );
}

#[test]
fn bare_query_string_yields_its_code() {
    assert_eq!(
        parse_pasted_input("code=abc123&state=na"),
        Ok("abc123".to_string())
    );
}

#[test]
fn url_without_code_is_used_whole() {
    let input = "https://tidal.com/android/login/auth?state=na";
    assert_eq!(parse_pasted_input(input), Ok(input.to_string()));
}

#[test]
fn error_yields_its_description() {
    let input = "https://tidal.com/android/login/auth?error=access_denied&error_description=The%20user%20cancelled+login";
    assert_eq!(
        parse_pasted_input(input),
        Err(PastedInputError::Denied(
            "The user cancelled login".to_string()
        ))
    );
}

#[test]
fn error_wins_over_code() {
    let input =
        "https://tidal.com/android/login/auth?code=abc&error=server_error&error_description=Oops";
    assert_eq!(
        parse_pasted_input(input),
        Err(PastedInputError::Denied("Oops".to_string()))
    );
}

#[test]
fn error_without_description_yields_the_error_code() {
    let input = "https://tidal.com/android/login/auth?error=access_denied";
    assert_eq!(
        parse_pasted_input(input),
        Err(PastedInputError::Denied("access_denied".to_string()))
    );
}

#[test]
fn blank_input_is_empty() {
    assert_eq!(parse_pasted_input(""), Err(PastedInputError::Empty));
    assert_eq!(parse_pasted_input(" \n\t "), Err(PastedInputError::Empty));
}
