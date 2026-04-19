public class Main {
    public static void main(String[] args) {
        UserService service = new UserService();
        User user = service.findById(1L);
        System.out.println(user.name());
    }
}
