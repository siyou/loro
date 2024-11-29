use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use loro::LoroDoc;

// https://gist.github.com/sunflowerdeath/c0e2b46b6f5d2e32d368f8e04f730237
fn main() {
    dev_utils::setup_test_log();
    let snapshot = "bG9ybwAAAAAAAAAAAAAAAK+fc78AA+8DAABMT1JPAAQiTRhgQIK0AwAA+loC29/O6L2r0tNnOMf35ey/nMj0vwE0AAwAZ6dJW90Tr9sAAAAAAAgACAEQAduvE91bSadnAQEAAAAAAAUBAAABAAYBBAECAAAFBHRleHQADgEEAgEAAgEAAgEFAgEIAAkISGVsbG9BYSBTAIQICA8XDwRJBFMA+m/crNGs++NE18d7mf3jIOm/FF2xSOwSfLEGAwMBAwEBBgMTAQIDAQIDAQIDAQEKAaLNqFRZtQqLNqEAAS4Cos8ABgEAAwAIAAYBBAECAAAFBHRleHQAEwEEAggABQM6HgQYAggFBAEGBgMAEwZBYSBBYSADQWEgA0FhIANBYSDvAHUXFwMpAwEwnAAEjAAMpADPAQEDBQECAwEoAqFPDwEE/gMPAQQCAQADAbgBAgEFAgEDAARvAIQaGgMyAwElAwsBDG8AEQH6AD8BNAFkAAwa0GQAVgIAdnYE0wH1CDaUusXFxN2EvrEBNtzZxua6/7ii1wE2/gHBOgAMALF8EuxIsV0U8wFkAwgDARsCjQAEnQARAQEAHw6LAAQE/gEbEO4AB1kASAMDAw5ZAATxAQFYAC8BClkADB8iWQAHhAYGDxcSBU8EsgAMrgEEwgD1AAMDAwMBBAEBCAMZAQIBAwMA+w0BCgSizaiUGcUSgziiUGcUcAEuA6EABgEABAAKWAL4BhIBBAIKAAYBOgYYAR4CCgUCCgMAFFACBAwAClQBXxUVBiwGVAEGHzL7AAT3BBABBAIEAAQDvgEGAgQFAgQDAAhfAIG/6SDj/Zl7x7MBZAYFCQIpA1IBBPEABBEB+wcDAQEEAQMBAgEIAZ6AAQoABgEAAQAE2wARD24ATwMDChJtAAt1BgYDEQMBJm0ABGUABHYBA7UCPwoBntcABAQrAhsu0gEH0QCECQkPFw8FTwTRAARcAARsAAThAA/SAQjvoU8oM+n1Bn0+oM+n1BDSAQcRENIBXwQBOggY0AEMB6AAjBgYAzIDARsCoAACywIfNPkABAQTBBrKrwOhDADXROP7rNGs3MsBAiUDBPIABGIAAloADyUDHQdZAIwDAxUUFQZVBFkABEsBBFsBEQZUAYkFAQEEAgYDGygD8gcCARAFoM+ndHnc7o87ndHnc7o4ASgEgwU7BQAMUAL/CBQBBAIMAAYFNBgSBhgCDAUEAQYKAwAbhAUAB9UCB7EASBgYAy8KAQShAAIKAR8uZAEMKcQBXgLwFAAWAGkABQF0AdgBCQJiArsCXQO8AykEjQQtBYcF4AWRBhEAAAAAAEcmhCABAAAABQAAAAIAZnIBDADXROP7rNGs3AAAABi9GZA8zAMAANwAAABMT1JPAAQiTRhgQIKjAAAAzwIBAG5IZWxsb0FhIAMAU/clBNuvE91bSadnFF2xSOwSfLHHe5n94yDpv9ys0az740TXAwQmBAAEAg0AAQQBAgUABgIBBQQA9w0DBAINAAUCAAQFBCYzAAoJAAYABQwFCgYJBgUQBAD2DQQGDwUGBAMGAAQDJwQAAxAFBgYjBQwDAAQFBgMEAPAKBAYJCwYAAgoEAAMCBAUDCgNEAAAAAAABAAAAAAAnKLzEAQAAAAUAAAAGAIIEdGV4dAEGAIIEdGV4dLBU8Ua7AAAAAAAAAA==";
    let update = "bG9ybwAAAAAAAAAAAAAAAC0ZfV0ABFUbAzUDASUD3KzRrPvjRNcUXbFI7BJ8scd7mf3jIOm/AQECAwECATQBAAAABQEAAAEABgEEAQIAAAUEdGV4dAAPAQQCAQADAdYBAgEFAgEDAAQDQWEg";
    let decoded_snapshot = STANDARD.decode(snapshot).unwrap();
    let decoded_update = STANDARD.decode(update).unwrap();

    println!(
        "snapshot vv:{:#?}",
        LoroDoc::decode_import_blob_meta(&decoded_snapshot, false)
            .unwrap()
            .partial_end_vv
    );
    let update_meta = LoroDoc::decode_import_blob_meta(&decoded_update, false).unwrap();
    dbg!(update_meta);
    let doc = LoroDoc::new();
    doc.import(&decoded_snapshot).unwrap();
    // doc.detach();
    println!("Imported snapshot");

    doc.import(&decoded_update).unwrap();
    println!("Imported update");
    doc.checkout_to_latest();
    doc.check_state_correctness_slow();
    let res = doc.export_json_updates(&Default::default(), &doc.oplog_vv());
    println!("{:#?}", serde_json::to_value(res).unwrap());
}