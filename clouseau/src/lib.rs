//! Clouseau is a quick and dirty in-memory, full-text search engine.
//!
//! It builds off of SQLite's in-memory capabilities (and full-text search),
//! acting as a simplified interface specifically for indexing and retrieving
//! objects.

#[macro_use]
extern crate quick_error;
extern crate rusqlite;

use ::std::error::Error;

use ::rusqlite::Connection;

//                          ....~?=:::~M8.+$??Z$DON??=Z+,+=~.....               
//           ...           ....~?IZO==+:=$+:+:?.$8=I.$~::+:=~....               
//           ....           ..+~$I:$$$7??MI$:N:???,$7=~I+=~:,,,.....            
//         ...             .,=+7+==+,~:I+:,,+I~~87I$7I+:ZO~+,:=,....            
//       ....              .+I=:.7Z~:8:N7?7$+=87:DD:,I=:=+8Z=,$=,...            
//     .....           .....~~?.$8D,NO~DZNMMDNMMMMNNIIIZ+=,I8+~::... .          
//   .....             ...,$7O=NDN?NM+DZ?O?==?+=++7?D88MNZZZ7M==~?....          
//   ...  .            ...77NM.=~=+,.=~,~=+~+?==7=,~==~???,7NNZ~~=~...          
//   ..                ...M,=:~,,~,~~~.8=I=+?:=+~:$:+~::~,==~++DN=:...          
//                 ....?.:,,,:~:.+:~:+~~:=~.,,===:::=::,~,~~+,::?O8:....        
//             .....,.~:~:=?7+Z7~$IOZN?7I8IM?ZOD7N8N$ZI$Z$ZO$I??,~=$7,....      
//             .....ZI87=7DDDNDDMNDMNNNDNDDND88O8O8O8OZZOO8DNN$D87?$:.....      
//             ..,ZZONNNNNM888OZOZZZOOOOOZ$7777$$7$$7777II$7$DNN8MO$$:.=..      
//             ..OID8DNNNNNZZZZ$77777$77I7777Z$$$$I77IIII77777DNNNNMMZ77~.....  
//             ..=8NNNNNNNN$$$Z$7$77II7III7ONN8DNNNDD??I?II777ONNNNNNMNZ?$+... .
//             ...$NNNNNNNN$$7DDDNDZ7?I?+II$Z$7Z77+??I?????II$ZONNNNNNMNMM=I....
//           .....,ONNNNNNNINMN8OOZ$$I?+?I7$77D8$8NI+?++++??IIZ8NNNNDNNNDDNN$~..
//      ...... .....ONNNNNNZ7I?$M88N7$+=+7I$7O=.N8M:7+?++????778NNNNDMNNNN8D$+..
// .................,8MNNNNNI+O~$=8~8Z?=+I?=?7?+II=??+++++++?IN$DNDI?7MDDNND+I..
// ............... ...8NNNNN~+?I:D:7I?~~=?=+++?7?7I+=~===++=?+87ODO7I8:NNND8=+..
// .................. .78NN8Z?77$7I??=::~++=~~=++=~~:~=======IZMDZ+I+Z=NNN8+=...
//     ..    .. .... . ..NNNO?++++??+=,:~==~=~~~==~~~~~~~====~IZ8IZ7=ZNNNN$7... 
//               ..... ...ZDD?=++++=+~:,,~=~~~~~~~===~=====~~==?II7Z+:MNNO?...  
//                .......,.M7======++:,,,~===~~~~~===++++===~~~=+?+ZONNDD$....  
//               ..  .    ...++++==++~:,::=:~?=~~~~==+++++==~~~=?$+=~8D8+.. .   
//     ... . ... ..        ...=+?+++++??I?:Z++~?=~~~========~~==?+=?8IN,.....   
//    ...,NMMMMMNMM?....   ...+?++++$$?77I=+I~,:+=~~~~=======~=~=~~=D~....      
//    ..NMO+~IMMNMNMDNI..  ...??+++?=~777OII+~~:~==~~========~~==7DD8.....      
// ...M:.,,,,,,,,.$DNNMMM ....:++++=~+$7878IO7?=~::~~~=~========?7N8.,,,..      
// ..D......,,,,,..,,NNMMM7....=+++?7787$7Z7OO$$$+=~~~~=======+=?7M:,,,,,.....  
// .M~....,,,,,,,,.,...NMMMN,,.~==+$D7II?+7I777+??+:~~~=======+?I8~,,,,,,,..    
// +N:..,,,,,,,,,,,,..,~7MMNM...+=+O77Z8Z$I=:+==7+~~~~~~===++???$,,,,,.,,,,...  
// M8,..,,,,,,,.,,,,,,..,.NMMN,..=+?II+=++I===~~==?+~~~~====??$$:,,,,,,,,,,,....
// MM...,,,,,,,.,,,,,,,....DDN8..~=+??+???III?+=~~~=+=====++I$7:.,,,,,,,,,,,,,..
// DD,.,,,,,,,.,,,,,,,,,...~ND8...~+=++?I???I??=+==~~===+++7Z7,:,,,,,....,,,,,..
// .N7,,,,,,,,.,,,,,:,,,,.,,MDNM...~=++?=~=~~~==~~====+??I$$==:,,,.,.,........,.
// .MD ..,,,,,.,,,,,,,,,,,,..NMM....~==~:~~~::~~====++?I$Z7,=,,,..,.......,,..,,
// ..DN:....,,..,,,,,,:,,,,.:MMM. ....==+===+===+++??7$$O~.=,,,,,.,.....,.......
//   ,MM.,,..,..,,,,,,,:,,,.,IMM......,++??????????$Z$Z.,.=,,,,,.,..............
//   .7MN:......,,,,:,,,,,..:7MI........=????77$$ZZZ$+,,,+,,,,,,,,.,..,.........
//   ..,MMM:....,,,,,,:,,....MN.       .=I$$$$$$$7$7::,==,,,,,,,..............:,
//     ..ONM7...,,,,,,,.....8N.........,:.7Z$7$7I?::,:?~:,,.......,,,........~:,
//     ...,MNNM.,,..,..,..,M=........,,,.,877777:,::7=+::,,,.....,,,........~:,,
//        .. ,NDNMD,.,,,,MD.,... ..::~,:.,,D:::,,,:?==~:,,...,.............:::,,
//         ......DMMNDN8$..........,,:,,.7D:,...,+==~?IM8.,.,..............::,..
//         .......,?NNMM.........O,,,,,.+$8D....=~~=,MNNND8=,,............~:,...
//         ...+ZZZOO8O888OI.. ...MZI.,~,?INN7..:~,:,:NZ77I7$ZN. ........,:,,....
//         ..IOOO8O888888DD$~:..+,:,,:=8D78MDI::,,,,.?=~~:~~++I$Z.,.....,,,,....
//         ..ZOO88DDND88OODDDZ?~..,.,=Z?NNZN.,,,,,,,,,+,,,,,,,:=+?I$...,,.......
//         ....ZMMNMMNNDNNNDDDO8==..,IDZ=DD.,,..,,...,.~,......,,~==+=:.........
//          .. .ZDO8DNDNMNNNNNNNOI+=~IIN8N..:,.......,,=~,......,,,:::,.........

quick_error! {
    #[derive(Debug)]
    pub enum CError {
        Boxed(err: Box<Error + Send + Sync>) {
            description(err.description())
            display("error: {}", err)
        }
    }
}
/// A macro to make it easy to create From impls for CError
macro_rules! from_err {
    ($t:ty) => (
        impl From<$t> for CError {
            fn from(err: $t) -> CError {
                CError::Boxed(Box::new(err))
            }
        }
    )
}
from_err!(::rusqlite::Error);
type CResult<T> = Result<T, CError>;

/// The Clouseau object stores all of our search state
pub struct Clouseau {
    /// Holds our sqlite connection DUUHHHHH
    pub conn: Connection,
}

impl Clouseau {
    /// Ahh, yees, the old "create a new struct and return it by value" ploy.
    /// Very clever. Very clever indeed!
    pub fn new() -> CResult<Clouseau> {
        let conn = Connection::open_in_memory()?;
        conn.execute("CREATE VIRTUAL TABLE objects USING fts4 (id VARCHAR(64) PRIMARY KEY, content TEXT)", &[])?;
        Ok(Clouseau {
            conn: conn,
        })
    }

    /// Index an object
    pub fn index(&self, id: &String, body: &String) -> CResult<()> {
        self.conn.execute("INSERT OR REPLACE INTO objects (id, content) VALUES (?, ?)", &[id, body])?;
        Ok(())
    }

    /// Remove an object from the index
    pub fn unindex(&self, id: &String) -> CResult<()> {
        self.conn.execute("DELETE FROM objects WHERE id = ?", &[id])?;
        Ok(())
    }

    /// Find things in the index
    pub fn find(&self, terms: &String) -> CResult<Vec<String>> {
        let mut query = self.conn.prepare("SELECT id FROM objects WHERE content match ? ORDER BY id ASC")?;
        let rows = query.query_map(&[terms], |row| {
            row.get("id")
        })?;
        let mut ids: Vec<String> = Vec::new();
        for id in rows { ids.push(id?) }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_has_ft() {
        Clouseau::new().unwrap();
    }

    #[test]
    fn searches_things() {
        let search = Clouseau::new().unwrap();
        search.index(&String::from("1111"), &String::from("what's the ugliest part of your body?")).unwrap();
        search.index(&String::from("1234"), &String::from("some say your nose")).unwrap();
        search.index(&String::from("2222"), &String::from("some say your toes")).unwrap();
        search.index(&String::from("3333"), &String::from("i think it's your mind")).unwrap();

        assert_eq!(search.find(&String::from("some say")).unwrap(), vec![String::from("1234"), String::from("2222")]);
        assert_eq!(search.find(&String::from("your some")).unwrap(), vec![String::from("1234"), String::from("2222")]);
        assert_eq!(search.find(&String::from("ugliest")).unwrap(), vec![String::from("1111")]);
        assert_eq!(search.find(&String::from("ugliest mind")).unwrap().len(), 0);
        assert_eq!(search.find(&String::from(r#""your some""#)).unwrap().len(), 0);

        search.unindex(&String::from("1234")).unwrap();
        assert_eq!(search.find(&String::from("some say")).unwrap(), vec![String::from("2222")]);
        search.unindex(&String::from("2222")).unwrap();
        assert_eq!(search.find(&String::from("some say")).unwrap().len(), 0);
    }

    #[test]
    fn index_large_document() {
        let search = Clouseau::new().unwrap();
        let body = String::from(r#"
YES A LOGO!!!!! That is what my website is missing!! I knew something was off
about my website, but I simple could not put my finger on it. I will certainly
Activate My Offer and I would like to order twenty of your _finest logos_.
Please have them sent directly to this email and I will certainly remit
payment after I have the logos.

Now, I know that your logo company specifically makes logos of various
cheeses, but I am going to request that you do logos of things OTHER than
cheese. I know this is a lot to ask of __Logo Cheese - USA__ but hear me out. When
I was but a young lad, my father used to take my brothers and myself horseback
riding into the Yorkshire hills. We would laugh and sing and eat assortments
of cheeses into the early evening. Then we would ride to my grandpapa's estate
and spend the week eating more cheese and chuckling over fresh cups of English
breakfast tea. Not the store-bought tea you find at the local grocers, being
bought by the common coupon-waving trash. No, we would have the _finest_
handmade teas with the most expensive ingredients delivered personally by the
craftsman himself, I think his name was Edward. No, it must have been
Bartholomew. I believe Edward was the local butcher, who would give us the
_finest_ cuts of beefs shoulder one could possibly eat!! The beef was from the
most expensive cows in all the land, and Edward would let us pick out the cow
and would butcher it, alive, right in front of us. It was delightful! You see,
if you kill a cow and _then_ butcher it, much of the flavor is lost. So we would
all take turns butchering the poor beast as Edward cheered us on! A truly
magnificent experience! Then Edward would package our meat and we would feast
that very night!!! We would eat our beef shoulder roasts at my grandpapa's
30-person dining table, waited on by his staff of servants, and then we would
sit by the fire and talk of of times past as we drank our English Breakfast
tea, hand-delivered by Bartholomew himself. Now, Bartholomew was a character!
The days he visited were some of the most exciting, because not only did he
craft and deliver our tea, but the man was a magician!! You can imagine how
wonderful that would be for a young lad, to drink his tea whilst watching a
magic show _right before him!!_ It was safe to say the Bartholomew was one of
our greatest companions!! I digress, though.

You see, one time, in the hillsides, as we were eating our artisanal cheeses
and laughing and singing, just before riding to my grandpapa's house and
spending the week drinking the _finest_ tea and eating the _finest_ beef shoulder
money can buy, we noticed a shadowy figure approaching from the Northern
hills. Years before, papa had instructed us never to go into the Northern
hills. There were stories of awful, sickly creatures there, but also of a
village deep in the forest where a group of bandits was exiled by King George
himself. As the tales go, the bandits had to choose either mating with each
other or with the various beasts roaming the hillsides for generations. You
can imagine the result! I personally once tried to mate with my father's prize
sheep, but the wretched thing would not sit still long enough. A man of my
stature does not take kindly to anyone, or anything, refusing him. Thus, I
relished sending _that_ awful sheep to the butcher one day as my father was away
on business. But that's another story!

As this shadowy figure approached, it became more and more grotesque in
appearance. Its shirt (if you can call it that!!) had a stain of some sort
right on the chest, and the hem around the trousers looked like it had come
undone _days_ ago! I couldn't help but feel sorry for the disgusting, vile
creature. As it came even closer though, I could make out its face. It was
Bartholomew!! I had never seen him look so disheveled. It made me want to
vomit. But papa says vomiting is for the peasants and the sickly, so I just
looked away in disgust instead and tried to think of my mother's fourty-acre
garden, instead of the monstrous image of Bartholomew, lurching through the
hillsides with stains on his shirt and tattered trousers.

My father got in between us and Bartholomew, protecting us from the vile
image. Bartholomew spoke: "HELLLLLP.....ME....." His raspy voice grated on my
ears. Must he keep speaking in that despicable voice? Drink a cup of tea, man!

"Really, Bartholomew," said father, bravely. "Get a hold of yourself man.
You're scaring the children You ought to be ashamed, wandering the hillside
looking like the common London street trash."

"HELLLLPP!"

"I certainly shall not! I refuse to help a man who will not help himself, who
staggers around in tattered clothing, expecting a hand out from those who work
hard for themselves. It goes without saying we will no longer be needing your
services at the estate, and I shall personally see to it that nobody else in
the town of Yorkshire ever buys tea from Bartholomew Dunscrup ever again!"

With that, father turned on his heel, gathered us onto the horses, and we set
off for grandapapa's house. But something was different this evening. The sky
was a deep maroon color and the air stank of flesh. We had only made it
halfway to grandpapa's house when the horses slowed, then stopped. Nothing we
could do would make them budge. We kicked and pushed, but they sat, still and
silent, as if they had given up, like that wretched man we once knew as
Bartholomew.. The thought of him sickened me.

Then it hit me. A hunger I cannot describe. It was not for the countryside's
finest beef shoulder. It was a deep hunger for something else. I could not
determine the cause of it until I saw my youngest brother's neck. My body
lurched for him, uncontrollable. Everything turned red. When I came to, hours
later (or so it felt), my brothers lay strewn across the hill, missing various
body parts. My shirt was covered in what looked like blood, and I had bits of
flesh between my teeth. What happened? I did not know. Someone had killed my
brothers, and from the looks of it had almost killed me. I looked into the
distance and saw a man running! I made chase. Perhaps this fine gentleman
could tell me of the events prior! Perhaps he witnessed this occurrence and
could help investigate!

As I gained on the gentleman, I noticed he had a familiar gait. It was father!
He looked back at me and screamed.

"Father, wait!" I shouted. But his pace only quickened. As I gained on him, I
noticed a familiar feeling creeping in. A hunger. It gave me an energy I had
not felt in the past, and my legs seemed move on their own, accelerating
beyond what I thought was possible. Just as I reached father, my vision turned
red again.

I woke up, in the dark, in a pool of father's blood. Whoever had murdered my
brothers had murdered father as well!! I swore vengeance to myself. You see, I
did not care much for my brothers, but father was very dear to me.

Then it struck me!! There was one other person in the hills that night. It was
Bartholomew! The vile man had obviously done this to father! I rushed back to
town and awoke the constable. He was a dear family friend, and as soon as he
heard what had happened, what Bartholomew had done, he rounded up the entire
police force and their most capable hounds, and we set off for an evening
hunt. I have always loved a good fox hunt, you see, but had never had the
opportunity to participate in a hunt at night!! The constable and I laughed
together as we spoke of previous hunts and how we would surely catch
Bartholomew on this eve!

Not a minute after we reached the hillside, the dogs picked up a scent. I knew
in my heart it was Bartholomew. We made haste and came to a clearing, lit only
by the moon, where we saw the same shadowy figure from before, on its knees,
crying into its hands. Aha! I thought to myself. We found the wretch!

We dismounted our horses and as we walked toward the figure, I recognized its
unnerving voice.

"HELLLP MEEE"

Oh, I would help it, certainly. I would help it shed its mortal coil and
release its vile soul back to the hell it came from. As I neared closer the
figure, I felt the same hunger from before. It must have been Bartholomew,
causing this odd feeling! It's proof! My vision went red again.

I awoke, but this time it was day. The entire hunting party, all their hounds,
and Bartholomew lay strewn before me, their chewed and ravaged corpses
beginning to cook slightly in the growing morning sun. Somehow Bartholomew had
killed all the policemen, but from the looks of it the dogs must have torn him
to shreds.

I searched the pockets of the creature, more disgusted by him than ever
before, and found that not only had he slain my brothers, my father, and the
entire Yorkshire police department, but he has _stolen cheese from my
grandfather!!_ 

I was in quite a rage at finding this, and you see, to this day, after
inheriting my father's wealth and my grandfather's estate, after living
through this horrid event and living to tell the tale, and after finding the
cheese in Bartholomew's pocket, I no longer can eat cheese.

Please consider this when sending the logos I have requested.
        "#);
        search.index(&String::from("1234"), &body).unwrap();
        search.index(&String::from("6969"), &String::from("ohhh. sayy. gnn dwnn blackbear")).unwrap();

        assert_eq!(search.find(&String::from(r#""website is missing""#)).unwrap(), vec!["1234"]);
        assert_eq!(search.find(&String::from(r#""website iz missing""#)).unwrap().len(), 0);
        assert_eq!(search.find(&String::from("certainly hillside creature")).unwrap(), vec!["1234"]);
        assert_eq!(search.find(&String::from("dogs shadowy running policemen grotesque coupon trash waving")).unwrap(), vec!["1234"]);
        assert_eq!(search.find(&String::from("blackbear")).unwrap(), vec!["6969"]);
        assert_eq!(search.find(&String::from("sand")).unwrap().len(), 0);
    }
}
