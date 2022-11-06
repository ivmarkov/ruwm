module box(width, height, depth, thickness, endingThickness, endingDepth, rdiam) {
    difference() {
        roundedCube(width, height, depth, rdiam);
        translate([0, 0, thickness]) roundedCube(width - thickness * 2, height - thickness * 2, depth, rdiam);
        translate([0, 0, (depth - endingDepth) / 2])
        difference() {
            roundedCube(width + 1, height + 1, endingDepth + 1, rdiam);
            roundedCube(width - thickness * 2 + endingThickness * 2, height - thickness * 2 + endingThickness * 2, endingDepth + 1, rdiam);
        };
    };
    
        
}

module roundedCube(width, height, depth, diam) {
    dwidth = width - diam;
    dheight = height - diam;
    
    hull() {
        translate([- dwidth / 2, - dheight / 2, 0]) cylinder(h = depth, d = diam, center = true);
        translate([dwidth / 2, dheight / 2, 0]) cylinder(h = depth, d = diam, center = true);
        translate([- dwidth / 2, dheight / 2, 0]) cylinder(h = depth, d = diam, center = true);
        translate([dwidth / 2, - dheight / 2, 0]) cylinder(h = depth, d = diam, center = true);
    }
}

module screws(width, height, depth, diam, screw) {
    fuse = 0.1;
    
    translate([- (width - screw) / 2 - fuse, - (height - screw) / 2 - fuse, 0]) 
    screwSupport(screw, screw, depth, diam);
    
    rotate(-90) 
    translate([- (height - screw) / 2 - fuse, - (width - screw) / 2 - fuse, 0]) 
    screwSupport(screw, screw, depth, diam);
    
    rotate(-180) 
    translate([- (width - screw) / 2 - fuse, - (height - screw) / 2 - fuse, 0]) 
    screwSupport(screw, screw, depth, diam);
    
    rotate(-270) 
    translate([- (height - screw) / 2 - fuse, - (width - screw) / 2 - fuse, 0]) 
    screwSupport(screw, screw, depth, diam);
}

module screwSupport(width, height, depth, diam) {
    $fn = 50;
    
    dwidth = width - diam;
    dheight = height - diam;
    
    hull() {
        translate([- dwidth / 2, - dheight / 2, 0]) cylinder(h = depth, d = diam, center = true);
        translate([dwidth / 2, dheight / 2, 0]) cube([diam, diam, depth], center = true);
        translate([- dwidth / 2, dheight / 2, 0]) cube([diam, diam, depth], center = true);
        translate([dwidth / 2, - dheight / 2, 0]) cube([diam, diam, depth], center = true);
    }
    //cube([width, height, depth], center = true);
}

module battery(
    width, 
    height, 
    depth, 
    batteryWidth,
    batteryHeight,
    batteryThickness, 
    supportThickness,
    screw,
    depthOffset
) {
    pdepth = depth - depthOffset;
    supWidth = supportThickness * 2;
    supHeight = supportThickness;
    supDepth = supportThickness * 3;
    
    translate([
        - (width - supWidth) / 2 + screw, 
        + (height - supHeight) / 2 - batteryThickness, 
        - (depth - supDepth) / 2
    ])
    cube([supWidth, supHeight, supDepth], center = true);

    translate([
        - (width - supWidth) / 2 + screw + batteryWidth - supWidth, 
        + (height - supHeight) / 2 - batteryThickness, 
        - (depth - supDepth) / 2
    ])
    cube([supWidth, supHeight, supDepth], center = true);

    translate([
        - (width - supHeight) / 2 + screw + batteryWidth,
        + (height - supHeight + screw) / 2 - batteryThickness, 
        - (depth - supDepth) / 2
    ])
    cube(
        [supHeight, supportThickness + screw, supDepth], 
        center = true);
    
//    translate([+ (iwidth - screw * 2 - thickness) / 2, - (iheight - screw * 2 - thickness) / 2 - 1, - (depth - screw) / 2 + thickness])
//    batterySupport(screw, thickness, screw);

//    translate([0, - (iheight - screw * 3) / 2 - 1, - depthOffset / 2])
//    cube([screw, screw, pdepth], center = true);
}

module screen(
    width, 
    height, 
    depth, 
    pcbWidth, 
    pcbHeight, 
    leftInset, 
    rightInset, 
    topInset, 
    bottomInset,
    screwsInset,
    screwsDiam
) {
    $fn = 50;
    
    screenWidth = pcbWidth - leftInset - rightInset;
    screenHeight = pcbHeight - topInset - bottomInset;
    drill = 20;
    
    translate([0, 0, - depth / 2])
    cube([screenWidth, screenHeight, drill], center = true);
    
    translate([(pcbWidth - screwsDiam) / 2 - 1, (pcbHeight - screwsDiam) / 2 - 1, - depth / 2])
    cylinder(h = drill, d = screwsDiam, center = true);

    translate([- (pcbWidth - screwsDiam) / 2 + 1, - (pcbHeight - screwsDiam) / 2 + 1, - depth / 2])
    cylinder(h = drill, d = screwsDiam, center = true);

    translate([+ (pcbWidth - screwsDiam) / 2 - 1, - (pcbHeight - screwsDiam) / 2 + 1, - depth / 2])
    cylinder(h = drill, d = screwsDiam, center = true);

    translate([- (pcbWidth - screwsDiam) / 2 + 1, + (pcbHeight - screwsDiam) / 2 - 1, - depth / 2])
    cylinder(h = drill, d = screwsDiam, center = true);
}

module roller(
    width,
    height,
    depth,
    offs,
    pcbWidth,
    pcbHeight,
    leftInset, 
    topInset, 
    rollerDiam
) {
    $fn = 50;
    
    drill = 20;

    rotate([0, 90, 0])
    translate([depth / 2 - leftInset + 1, height / 2 - offs + 1 - topInset, - width / 2])
    cylinder(h = drill, d = rollerDiam, center = true);
}

module antenna(
    width,
    height,
    depth,
    offs,
    diam,
    inset
) {
    $fn = 50;
    
    drill = 20;

    rotate([0, 90, 0])
    translate([depth / 2 - inset + 1 - diam, height / 2 - diam / 2 - inset - offs, + width  / 2])
    cylinder(h = drill, d = diam, center = true);
}

module mainPcb(
    width,
    height,
    depth,
    supportThickness,
    screw,
    pcbWidth,
    pcbHeight,
    pcbThickness,
    pcbScrewInset,
    pcbScrewDiam,
    pcbWallOffset
) {
    supWidth = pcbThickness + pcbWallOffset + 1;
    supHeight = pcbScrewDiam + 1;
    supDepth = supportThickness * 2;
    
    translate([
        + (width - supWidth) / 2, 
        - (height - supHeight) / 2 + screw - 0.1, 
        - (depth - supDepth) / 2
    ])
    difference() {
        cube([supWidth, supHeight, supDepth], center = true);
        translate([-1, 0, 0])
        cube([pcbThickness, supHeight + 0.1, supDepth + 0.1], center = true);
    }

    translate([
        + (width - supWidth) / 2, 
        - (height - supHeight) / 2 + screw - 0.1 + pcbWidth,
        - (depth - supDepth) / 2
    ])
    difference() {
        cube([supWidth, supHeight, supDepth], center = true);
        translate([-1, 0, 0])
        cube([pcbThickness, supHeight + 0.1, supDepth + 0.1], center = true);
    }

//    translate([(iwidth - supWidth) / 2, - (iheight - supHeight) / 2 + screw - 1, - (depth - supDepth) / 2 + thickness])
//    difference() {
//        cube([supWidth * 2, supHeight * 2, supDepth], center = true);
//        cube([pcbTickness, supHeight * 2, supDepth], center = true);
//    }
}

module wm(
    width, 
    height, 
    depth, 
    thickness, 
    endingThickness, 
    endingDepth, 
    diam, 
    screw, 
    depthOffset,
    screenPcbWidth,
    screenPcbHeight,
    screenLeftInset, 
    screenRightInset, 
    screenTopInset, 
    screenBottomInset,
    screenScrewsInset,
    screenScrewsDiam,
    rollerPcbWidth,
    rollerPcbHeight,
    rollerLeftInset, 
    rollerTopInset, 
    rollerDiam,
    antennaDiam,
    antennaInset,
    batteryWidth,
    batteryHeight,
    batteryThickness,
    batterySupportThickness,
    mainPcbWidth, 
    mainPcbHeight, 
    mainPcbTickness,
    mainPcbScrewInset,
    mainPcbScrewDiam,
    mainPcbWallOffset
) {
    
    iwidth = width - thickness * 2;
    iheight = height - thickness * 2;
    idepth = depth - thickness * 2;
    
    difference() {
        box(width, height, depth, thickness, endingThickness, endingDepth, diam);

        screen(
            iwidth, 
            iheight, 
            idepth, 
            screenPcbWidth, 
            screenPcbHeight,
            screenLeftInset,
            screenRightInset,
            screenTopInset,
            screenBottomInset,
            screenScrewsInset,
            screenScrewsDiam
        );
        
        roller(
            iwidth,
            iheight,
            idepth,
            batteryThickness + batterySupportThickness + 1,
            rollerPcbWidth, 
            rollerPcbHeight,
            rollerLeftInset,
            rollerTopInset,
            rollerDiam
        );
        
        antenna(
            iwidth,
            iheight,
            idepth,
            batteryThickness + batterySupportThickness + 1,
            antennaDiam,
            antennaInset
        );
    };
    
    screws(iwidth, iheight, depth, diam, screw);
    
    battery(
        iwidth, 
        iheight, 
        idepth, 
        batteryWidth,
        batteryHeight,
        batteryThickness, 
        batterySupportThickness, 
        screw, 
        depthOffset
    );
    
    mainPcb(
        iwidth, 
        iheight, 
        idepth, 
        batterySupportThickness,
        screw,
        mainPcbWidth, 
        mainPcbHeight, 
        mainPcbTickness,
        mainPcbScrewInset,
        mainPcbScrewDiam,
        mainPcbWallOffset
    );
}

wm(
    width = 71,
    height = 63,
//    50, // depth
    depth = 25,
    thickness = 2,
    endingThickness = 1,
    endingDepth = 3,
    diam = 4,
    screw = 6,
    depthOffset = 10,
    screenPcbWidth = 45,
    screenPcbHeight = 37,
    screenLeftInset = 8,
    screenRightInset = 8,
    screenTopInset = 2,
    screenBottomInset = 5,
    screenScrewsInset = 1,
    screenScrewsDiam = 3,
    rollerPcbWidth = 26,
    rollerPcbHeight = 20,
    rollerLeftInset = 12,
    rollerTopInset = 9,
    rollerDiam = 8,
    antennaDiam = 7,
    antennaInset = 3,
    batteryWidth = 50,
    batteryHeight = 60,
    batteryThickness = 7,
    batterySupportThickness = 2,
    mainPcbWidth = 29,
    mainPcbHeight = 58,
    mainPcbTickness = 2,
    mainPcbScrewInset = 1,
    mainPcbScrewDiam = 3,
    mainPcbWallOffset = 3
);
