pub(super) trait IconReferences {
    fn append_icons<'a>(&'a self, kind: &'a str, icons: &mut Vec<(&'a str, &'a str, &'static str)>);
}

pub(super) trait ImageReference {
    fn append_image<'a>(
        &'a self,
        kind: &'a str,
        field: &'static str,
        icons: &mut Vec<(&'a str, &'a str, &'static str)>,
    );
}

impl ImageReference for String {
    fn append_image<'a>(
        &'a self,
        kind: &'a str,
        field: &'static str,
        icons: &mut Vec<(&'a str, &'a str, &'static str)>,
    ) {
        icons.push((kind, self, field));
    }
}

impl<T: ImageReference> ImageReference for Option<T> {
    fn append_image<'a>(
        &'a self,
        kind: &'a str,
        field: &'static str,
        icons: &mut Vec<(&'a str, &'a str, &'static str)>,
    ) {
        if let Some(image) = self {
            image.append_image(kind, field, icons);
        }
    }
}

impl<T: IconReferences> IconReferences for Option<T> {
    fn append_icons<'a>(&'a self, kind: &'a str, icons: &mut Vec<(&'a str, &'a str, &'static str)>) {
        if let Some(icon) = self {
            icon.append_icons(kind, icons);
        }
    }
}

impl<T: IconReferences> IconReferences for Vec<T> {
    fn append_icons<'a>(&'a self, kind: &'a str, icons: &mut Vec<(&'a str, &'a str, &'static str)>) {
        for record in self {
            record.append_icons(kind, icons);
        }
    }
}

pub(super) trait SocketOptionIcons {
    fn append_option_icons<'a>(&'a self, kind: &'a str, icons: &mut Vec<(&'a str, &'a str, &'static str)>);
}

impl<T: IconReferences> SocketOptionIcons for Vec<T> {
    fn append_option_icons<'a>(&'a self, kind: &'a str, icons: &mut Vec<(&'a str, &'a str, &'static str)>) {
        self.append_icons(if kind == "items" { "augments" } else { kind }, icons);
    }
}

impl SocketOptionIcons for String {
    fn append_option_icons<'a>(&'a self, _: &'a str, _: &mut Vec<(&'a str, &'a str, &'static str)>) {}
}
