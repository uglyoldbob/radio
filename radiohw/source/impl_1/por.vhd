library IEEE;
use IEEE.STD_LOGIC_1164.ALL;
use IEEE.NUMERIC_STD.ALL;

entity por is
   Generic(
        cycles: in integer := 5);
   Port (
		rst_n: in std_logic := '1';
		reset: out std_logic;
		clock: in std_logic);
end por;

architecture Behavioral of por is
	signal reset_delay: std_logic_vector(cycles-1 downto 0) := (others => '0');
begin
	process (clock, rst_n)
	begin
		if rising_edge(clock) then
			if rst_n = '0' then
				reset_delay <= (others => '0');
			else
				reset_delay <= "1" & reset_delay(cycles-1 downto 1);
			end if;
		end if;
	end process;
	process (all)
	begin
		if and reset_delay then
			reset <= '1';
		else
			reset <= '0';
		end if;
	end process;
end Behavioral;

